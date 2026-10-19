mod documents;
mod revision;
mod state;

use crate::{
    AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig, ProjectError,
    ProjectResolver, SourceDocument, query::QueryStore,
};
use state::SessionState;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable};

pub use revision::DatabaseRevision;

/// Central incremental analysis database behind [`AnalysisHost`](crate::AnalysisHost).
///
/// Owns document overlays, opaque summaries, and the private Salsa inputs.
/// Mutations bump a revision; the next snapshot publishes changed inputs.
/// Reads go through an immutable [`AnalysisSnapshot`] and honor cooperative
/// cancellation. Salsa is the only cross-request memo owner.
///
/// # Main usage flow
///
/// ```text
///  ProjectResolver + overlays + opaque summaries
///              |
///              v
///    +---------------------+
///    |  AnalysisDatabase   |  only mutable input entry
///    +----------+----------+
///               |
///     mutations (revision++):
///       load / set_roots / open|update|close_document
///       set_opaque_summaries / register_opaque_summary
///               |
///               v
///     snapshot[ _with_token ]  -->  publish inputs, then AnalysisSnapshot
///               |
///               v
///     consumers: syl_query::AnalysisQueries, hwir(), diagnostics, ...
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct AnalysisDatabase {
    state: SessionState,
    query: QueryStore,
    live: Arc<AtomicUsize>,
}

impl AnalysisDatabase {
    pub fn new() -> Self {
        Self::with_config(ProjectConfig::new())
    }

    pub fn with_config(config: ProjectConfig) -> Self {
        Self::from_state(SessionState::with_config(config))
    }

    pub fn with_resolver(resolver: ProjectResolver) -> Self {
        Self::from_state(SessionState::with_resolver(resolver))
    }

    fn from_state(state: SessionState) -> Self {
        Self {
            state,
            query: QueryStore::new(),
            live: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn load(&mut self, inputs: &[PathBuf]) -> Result<AnalysisSnapshot, ProjectError> {
        self.load_with_token(inputs, &CancellationToken::new())
    }

    pub fn load_with_token(
        &mut self,
        inputs: &[PathBuf],
        token: &CancellationToken,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        self.state.load_inputs(inputs)?;
        self.snapshot_with_token(token)
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.state.set_roots(roots);
    }

    pub fn roots(&self) -> &[PathBuf] {
        self.state.roots()
    }

    pub fn open_document(
        &mut self,
        uri: DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> DocumentVersion {
        self.state.open_document(uri, text, version)
    }

    pub fn update_document(
        &mut self,
        uri: &DocumentUri,
        text: String,
    ) -> Result<DocumentVersion, ProjectError> {
        self.state.update_document(uri, text)
    }

    pub fn update_document_at_version(
        &mut self,
        uri: &DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> Result<DocumentVersion, ProjectError> {
        self.state.update_document_at_version(uri, text, version)
    }

    pub fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        self.state.close_document(uri)
    }

    pub fn overlay(&self, uri: &DocumentUri) -> Option<&SourceDocument> {
        self.state.overlay(uri)
    }

    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        self.state.opaque_summaries()
    }

    pub fn set_opaque_summaries(&mut self, opaque_summaries: OpaqueSummaryTable) {
        self.state.set_opaque_summaries(opaque_summaries);
    }

    pub fn register_opaque_summary(&mut self, summary: OpaqueItemSummary) {
        self.state.register_opaque_summary(summary);
    }

    pub fn revision(&self) -> DatabaseRevision {
        self.state.revision()
    }

    pub fn snapshot(&mut self) -> Result<AnalysisSnapshot, ProjectError> {
        self.snapshot_with_token(&CancellationToken::new())
    }

    pub fn snapshot_with_token(
        &mut self,
        token: &CancellationToken,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        if token.is_cancelled() {
            return Err(ProjectError::Cancelled);
        }
        self.publish_snapshot(token)
    }

    fn publish_snapshot(
        &mut self,
        token: &CancellationToken,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        let resolved = self.state.resolve(token)?;
        let specs = resolved.file_specs();
        self.query.publish(&specs, self.state.opaque_summaries());
        Ok(AnalysisSnapshot::from_query(
            self.query.generation(),
            self.query.inflight(),
            crate::query::LiveGuard::enter(&self.live),
            resolved,
        ))
    }
}

impl Default for AnalysisDatabase {
    fn default() -> Self {
        Self::new()
    }
}
