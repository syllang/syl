use crate::{
    AnalysisDatabase, AnalysisSnapshot, DocumentUri, DocumentVersion, ProjectConfig, ProjectError,
    ProjectResolver, SourceDocument,
};
use std::path::PathBuf;
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable};

#[derive(Debug)]
#[non_exhaustive]
pub struct AnalysisHost {
    database: AnalysisDatabase,
}

impl AnalysisHost {
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
}

impl Default for AnalysisHost {
    fn default() -> Self {
        Self::new()
    }
}
