mod documents;
#[cfg(test)]
mod reconfigure_tests;
mod revision;
mod state;

use crate::{
    AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig, ProjectError,
    ProjectManifest, ProjectResolver, SourceDocument, TextDocumentEdit, query::QueryStore,
};
use state::SessionState;
use std::path::PathBuf;
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
}

impl AnalysisDatabase {
    /// Replace configuration and summaries only after all manifest metadata loads.
    /// Open document overlays are preserved, including their versions.
    pub fn apply_manifest(&mut self, manifest: &ProjectManifest) -> Result<(), ProjectError> {
        let summaries = manifest.load_opaque_summaries()?;
        self.reconfigure(manifest.to_project_config());
        self.set_opaque_summaries(summaries);
        Ok(())
    }

    /// Replace resolver configuration while preserving overlays and summaries.
    pub fn reconfigure(&mut self, config: ProjectConfig) {
        self.state.reconfigure(config);
    }

    /// Remove registered opaque summaries without importing their representation.
    pub fn clear_opaque_summaries(&mut self) {
        self.set_opaque_summaries(OpaqueSummaryTable::new());
    }

    /// Refresh disk roots without building a snapshot; unchanged roots are a no-op.
    pub fn refresh_inputs(&mut self, inputs: &[PathBuf]) -> Result<(), ProjectError> {
        self.state.load_inputs(inputs)
    }

    /// Apply a sequential UTF-16 edit batch atomically to an open document.
    pub fn apply_document_changes_at_version(
        &mut self,
        uri: &DocumentUri,
        changes: &[TextDocumentEdit],
        version: DocumentVersion,
    ) -> Result<DocumentVersion, ProjectError> {
        self.state.apply_edits(uri, changes, version)
    }

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
        let resolved = self.state.resolve_into_query(&mut self.query, token)?;
        Ok(AnalysisSnapshot::from_query(
            self.query.generation(),
            self.query.inflight(),
            resolved,
        ))
    }
}

impl Default for AnalysisDatabase {
    fn default() -> Self {
        Self::new()
    }
}
