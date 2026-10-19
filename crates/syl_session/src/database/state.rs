//! Mutable session inputs and their revision lifecycle.
//!
//! `AnalysisDatabase` owns the query store and orchestrates publication. This
//! object owns the application state that can change between snapshots: roots,
//! overlays, the resolver, and opaque summaries. Keeping those concerns
//! together gives each mutation one natural owner and keeps the database
//! orchestration small.

use super::documents::DocumentStore;
use super::revision::DatabaseRevision;
use crate::{
    CancellationToken, DocumentUri, DocumentVersion, ProjectConfig, ProjectError, ProjectResolver,
    SourceDocument, collector::SylFileCollector, snapshot::ResolvedSnapshot,
};
use std::path::PathBuf;
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable};

#[derive(Debug)]
pub(crate) struct SessionState {
    resolver: ProjectResolver,
    documents: DocumentStore,
    opaque_summaries: OpaqueSummaryTable,
    revision: DatabaseRevision,
}

impl SessionState {
    pub(crate) fn with_config(config: ProjectConfig) -> Self {
        Self::with_resolver(ProjectResolver::with_config(config))
    }

    pub(crate) fn with_resolver(resolver: ProjectResolver) -> Self {
        Self {
            resolver,
            documents: DocumentStore::default(),
            opaque_summaries: OpaqueSummaryTable::new(),
            revision: DatabaseRevision::initial(),
        }
    }

    pub(crate) fn load_inputs(&mut self, inputs: &[PathBuf]) -> Result<(), ProjectError> {
        let mut roots = Vec::new();
        let mut collector = SylFileCollector::new(&mut roots);
        for input in inputs {
            collector.collect(input)?;
        }
        self.set_roots(roots);
        Ok(())
    }

    pub(crate) fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.documents.set_roots(roots);
        self.touch();
    }

    pub(crate) fn roots(&self) -> &[PathBuf] {
        self.documents.roots()
    }

    pub(crate) fn open_document(
        &mut self,
        uri: DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> DocumentVersion {
        self.documents.open_document(uri, text, version);
        self.touch();
        version
    }

    pub(crate) fn update_document(
        &mut self,
        uri: &DocumentUri,
        text: String,
    ) -> Result<DocumentVersion, ProjectError> {
        let version = self.documents.next_document_version(uri)?;
        self.update_document_at_version(uri, text, version)
    }

    pub(crate) fn update_document_at_version(
        &mut self,
        uri: &DocumentUri,
        text: String,
        version: DocumentVersion,
    ) -> Result<DocumentVersion, ProjectError> {
        self.documents.update_document(uri, text, version)?;
        self.touch();
        Ok(version)
    }

    pub(crate) fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        let removed = self.documents.close_document(uri);
        if removed.is_some() {
            self.touch();
        }
        removed
    }

    pub(crate) fn overlay(&self, uri: &DocumentUri) -> Option<&SourceDocument> {
        self.documents.overlay(uri)
    }

    pub(crate) fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        &self.opaque_summaries
    }

    pub(crate) fn set_opaque_summaries(&mut self, opaque_summaries: OpaqueSummaryTable) {
        if self.opaque_summaries == opaque_summaries {
            return;
        }
        self.opaque_summaries = opaque_summaries;
        self.touch();
    }

    pub(crate) fn register_opaque_summary(&mut self, summary: OpaqueItemSummary) {
        let mut opaque_summaries = self.opaque_summaries.clone();
        opaque_summaries.register(summary);
        self.set_opaque_summaries(opaque_summaries);
    }

    pub(crate) fn revision(&self) -> DatabaseRevision {
        self.revision
    }

    pub(crate) fn resolve(
        &self,
        token: &CancellationToken,
    ) -> Result<ResolvedSnapshot, ProjectError> {
        let inputs = self.documents.snapshot_inputs();
        let (roots, overlays) = inputs.into_resolver_inputs();
        self.resolver.snapshot(roots, overlays, token)
    }

    fn touch(&mut self) {
        self.revision = self.revision.next();
    }
}
