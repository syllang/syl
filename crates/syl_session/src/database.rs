mod documents;
mod revision;

use crate::{
    AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig, ProjectError,
    ProjectResolver, SourceDocument, collector::SylFileCollector, query::QueryStore,
};
use documents::DocumentStore;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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
    resolver: ProjectResolver,
    documents: DocumentStore,
    opaque_summaries: OpaqueSummaryTable,
    query: QueryStore,
    revision: DatabaseRevision,
    live: Arc<AtomicUsize>,
}

impl AnalysisDatabase {
    pub fn new() -> Self {
        Self::with_config(ProjectConfig::new())
    }

    pub fn with_config(config: ProjectConfig) -> Self {
        Self::with_resolver(ProjectResolver::with_config(config))
    }

    pub fn with_resolver(resolver: ProjectResolver) -> Self {
        Self {
            resolver,
            documents: DocumentStore::default(),
            opaque_summaries: OpaqueSummaryTable::new(),
            query: QueryStore::new(),
            revision: DatabaseRevision::initial(),
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
        let mut roots = Vec::new();
        {
            let mut collector = SylFileCollector::new(&mut roots);
            for input in inputs {
                collector.collect(input)?;
            }
        }
        self.set_roots(roots);
        self.snapshot_with_token(token)
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.documents.set_roots(roots);
        self.advance_revision();
    }

    pub fn roots(&self) -> &[PathBuf] {
        self.documents.roots()
    }

    pub fn open_document(
        &mut self,
        uri: DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> DocumentVersion {
        self.documents.open_document(uri, text, version);
        self.advance_revision();
        version
    }

    pub fn update_document(
        &mut self,
        uri: &DocumentUri,
        text: String,
    ) -> Result<DocumentVersion, ProjectError> {
        let version = self.documents.next_document_version(uri)?;
        self.update_document_at_version(uri, text, version)
    }

    pub fn update_document_at_version(
        &mut self,
        uri: &DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> Result<DocumentVersion, ProjectError> {
        self.documents.update_document(uri, text, version)?;
        self.advance_revision();
        Ok(version)
    }

    pub fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        let removed = self.documents.close_document(uri);
        if removed.is_some() {
            self.advance_revision();
        }
        removed
    }

    pub fn overlay(&self, uri: &DocumentUri) -> Option<&SourceDocument> {
        self.documents.overlay(uri)
    }

    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        &self.opaque_summaries
    }

    pub fn set_opaque_summaries(&mut self, opaque_summaries: OpaqueSummaryTable) {
        if self.opaque_summaries == opaque_summaries {
            return;
        }
        self.opaque_summaries = opaque_summaries;
        self.advance_revision();
    }

    pub fn register_opaque_summary(&mut self, summary: OpaqueItemSummary) {
        let mut opaque_summaries = self.opaque_summaries.clone();
        opaque_summaries.register(summary);
        self.set_opaque_summaries(opaque_summaries);
    }

    pub fn revision(&self) -> DatabaseRevision {
        self.revision
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
        let inputs = self.documents.snapshot_inputs();
        let (roots, overlays) = inputs.into_resolver_inputs();
        let resolved = self.resolver.snapshot(roots, overlays, token)?;
        let specs = resolved.file_specs();
        let live_count = self.live.load(Ordering::SeqCst);
        self.query
            .publish(&specs, &self.opaque_summaries, live_count);
        Ok(AnalysisSnapshot::from_query(
            self.query.generation(),
            self.query.inflight(),
            crate::query::LiveGuard::enter(&self.live),
            resolved,
        ))
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.next();
    }
}

impl Default for AnalysisDatabase {
    fn default() -> Self {
        Self::new()
    }
}
