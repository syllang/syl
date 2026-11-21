use crate::{
    AnalysisDatabase, AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion,
    ProjectConfig, ProjectError, ProjectManifest, ProjectResolver, SourceDocument,
    TextDocumentEdit,
};
use std::path::PathBuf;
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable};

/// High-level entry point for project-level analysis (CLI / tests / tools).
///
/// Thin façade over [`AnalysisDatabase`]: load roots, manage document overlays
/// and opaque summaries, then take immutable [`AnalysisSnapshot`]s for queries
/// or emission.
///
/// # Main usage flow
///
/// ```text
///  ProjectConfig / ProjectResolver / paths
///              |
///              v
///    +---------------------+
///    |    AnalysisHost     |   new() | with_config | with_resolver
///    +----------+----------+
///               |
///     +---------+-----------------------------+
///     |                                       |
///     v                                       v
///  load[ _with_token ](paths)     open/update/close_document
///  set_roots / register_opaque_*  (editor overlay mutations)
///               |
///               v
///  snapshot[ _with_token ]()
///               |
///               v
///       AnalysisSnapshot
///               |
///     +---------+----------+------------------+
///     |                    |                  |
///     v                    v                  v
///  hir_analysis()     tir_analysis()      hwir()
///  diagnostics...     AnalysisQueries     SystemVerilogBackend
///                     (via syl_query)
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct AnalysisHost {
    database: AnalysisDatabase,
}

impl AnalysisHost {
    /// Atomically apply manifest configuration and summaries, preserving overlays.
    pub fn apply_manifest(&mut self, manifest: &ProjectManifest) -> Result<(), ProjectError> {
        self.database.apply_manifest(manifest)
    }

    /// Replace resolver configuration while preserving overlays and summaries.
    pub fn reconfigure(&mut self, config: ProjectConfig) {
        self.database.reconfigure(config);
    }

    /// Clear all registered opaque summaries; an empty table is a no-op.
    pub fn clear_opaque_summaries(&mut self) {
        self.database.clear_opaque_summaries();
    }

    /// Re-collect disk file roots without eagerly constructing a snapshot.
    pub fn refresh_inputs(&mut self, inputs: &[PathBuf]) -> Result<(), ProjectError> {
        self.database.refresh_inputs(inputs)
    }

    /// Apply edits in order, committing only if the entire batch is valid.
    pub fn apply_document_changes_at_version(
        &mut self,
        uri: &DocumentUri,
        changes: &[TextDocumentEdit],
        version: DocumentVersion,
    ) -> Result<DocumentVersion, ProjectError> {
        self.database
            .apply_document_changes_at_version(uri, changes, version)
    }

    /// Construct a host from a manifest, including its trusted opaque summaries.
    pub fn from_manifest(manifest: &ProjectManifest) -> Result<Self, ProjectError> {
        let mut host = Self::with_config(manifest.to_project_config());
        host.load_manifest_opaque_summaries(manifest)?;
        Ok(host)
    }

    /// Merge manifest summaries into existing overlays, publishing only changes.
    pub fn load_manifest_opaque_summaries(
        &mut self,
        manifest: &ProjectManifest,
    ) -> Result<(), ProjectError> {
        let loaded = manifest.load_opaque_summaries()?;
        let mut summaries = self.opaque_summaries().clone();
        summaries.merge_from(&loaded);
        self.set_opaque_summaries(summaries);
        Ok(())
    }

    pub fn new() -> Self {
        Self::with_config(ProjectConfig::new())
    }

    pub fn with_config(config: ProjectConfig) -> Self {
        Self::with_resolver(ProjectResolver::with_config(config))
    }

    pub fn with_resolver(resolver: ProjectResolver) -> Self {
        Self {
            database: AnalysisDatabase::with_resolver(resolver),
        }
    }

    pub fn load(&mut self, inputs: &[PathBuf]) -> Result<AnalysisSnapshot, ProjectError> {
        self.database.load(inputs)
    }

    pub fn load_with_token(
        &mut self,
        inputs: &[PathBuf],
        token: &CancellationToken,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        self.database.load_with_token(inputs, token)
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.database.set_roots(roots);
    }

    pub fn roots(&self) -> &[PathBuf] {
        self.database.roots()
    }

    pub fn open_document(
        &mut self,
        uri: DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> DocumentVersion {
        self.database.open_document(uri, text, version)
    }

    pub fn update_document(
        &mut self,
        uri: &DocumentUri,
        text: String,
    ) -> Result<DocumentVersion, ProjectError> {
        self.database.update_document(uri, text)
    }

    pub fn update_document_at_version(
        &mut self,
        uri: &DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> Result<DocumentVersion, ProjectError> {
        self.database.update_document_at_version(uri, text, version)
    }

    pub fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        self.database.close_document(uri)
    }

    pub fn overlay(&self, uri: &DocumentUri) -> Option<&SourceDocument> {
        self.database.overlay(uri)
    }

    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        self.database.opaque_summaries()
    }

    pub fn set_opaque_summaries(&mut self, opaque_summaries: OpaqueSummaryTable) {
        self.database.set_opaque_summaries(opaque_summaries);
    }

    pub fn register_opaque_summary(&mut self, summary: OpaqueItemSummary) {
        self.database.register_opaque_summary(summary);
    }

    pub fn database(&self) -> &AnalysisDatabase {
        &self.database
    }

    pub fn database_mut(&mut self) -> &mut AnalysisDatabase {
        &mut self.database
    }

    pub fn snapshot(&mut self) -> Result<AnalysisSnapshot, ProjectError> {
        self.database.snapshot()
    }

    pub fn snapshot_with_token(
        &mut self,
        token: &CancellationToken,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        self.database.snapshot_with_token(token)
    }
}

impl Default for AnalysisHost {
    fn default() -> Self {
        Self::new()
    }
}
