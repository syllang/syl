mod cache;
mod documents;
mod revision;

use crate::{
    AnalysisSnapshot, DocumentUri, DocumentVersion, ProjectConfig, ProjectError, ProjectResolver,
    SourceDocument, collector::SylFileCollector,
};
use cache::{
    CachedSnapshot, DocumentKey, InvalidationPlan, SemanticCacheStore, SemanticSnapshotKey,
    SnapshotCache, SnapshotKey,
};
use documents::{DocumentInputs, DocumentStore};
use std::path::PathBuf;

pub use revision::DatabaseRevision;

#[derive(Debug)]
#[non_exhaustive]
pub struct AnalysisDatabase {
    resolver: ProjectResolver,
    documents: DocumentStore,
    snapshot_cache: SnapshotCache,
    semantic_cache_store: SemanticCacheStore,
    revision: DatabaseRevision,
}

#[derive(Debug)]
struct SnapshotQuery<'a> {
    key: SnapshotKey,
    inputs: DocumentInputs<'a>,
}

impl<'a> SnapshotQuery<'a> {
    fn new(inputs: DocumentInputs<'a>) -> Self {
        let key = inputs.snapshot_key();
        Self { key, inputs }
    }

    fn execute(
        self,
        resolver: &ProjectResolver,
        snapshot_cache: &mut SnapshotCache,
        semantic_cache_store: &mut SemanticCacheStore,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        if let Some(snapshot) = snapshot_cache.lookup(&self.key) {
            return Ok(snapshot);
        }

        let (roots, overlays) = self.inputs.into_resolver_inputs();
        let resolved = resolver.snapshot(roots, overlays)?;
        let semantic_key = SemanticSnapshotKey::from_snapshot(&resolved);
        let semantic =
            semantic_cache_store.semantic_for_snapshot(semantic_key, resolved.ast_files());
        let snapshot = AnalysisSnapshot::new(resolved, semantic);
        let cached = CachedSnapshot::new(self.key.clone(), snapshot);
        Ok(snapshot_cache.store(cached))
    }
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
            snapshot_cache: SnapshotCache::default(),
            semantic_cache_store: SemanticCacheStore::new(),
            revision: DatabaseRevision::initial(),
        }
    }

    pub fn load(&mut self, inputs: &[PathBuf]) -> Result<AnalysisSnapshot, ProjectError> {
        let mut roots = Vec::new();
        {
            let mut collector = SylFileCollector::new(&mut roots);
            for input in inputs {
                collector.collect(input)?;
            }
        }
        self.set_roots(roots);
        self.snapshot()
    }

    pub fn set_roots(&mut self, roots: Vec<PathBuf>) {
        self.documents.set_roots(roots);
        self.advance_revision();
        self.invalidate(InvalidationPlan::project_graph_changed());
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
        if let Some(previous) = self.documents.open_document(uri, text, version) {
            self.advance_revision();
            self.invalidate(InvalidationPlan::document_changed(previous));
        } else {
            self.advance_revision();
        }
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
        if let Some(previous) = self.documents.update_document(uri, text, version)? {
            self.advance_revision();
            self.invalidate(InvalidationPlan::document_changed(previous));
        } else {
            self.advance_revision();
        }
        Ok(version)
    }

    pub fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        let removed = self.documents.close_document(uri);
        if let Some(document) = removed.as_ref() {
            self.advance_revision();
            self.invalidate(InvalidationPlan::document_changed(
                DocumentKey::from_document(document),
            ));
        }
        removed
    }

    pub fn overlay(&self, uri: &DocumentUri) -> Option<&SourceDocument> {
        self.documents.overlay(uri)
    }

    pub fn revision(&self) -> DatabaseRevision {
        self.revision
    }

    pub fn snapshot(&mut self) -> Result<AnalysisSnapshot, ProjectError> {
        let query = SnapshotQuery::new(self.documents.snapshot_inputs());
        let resolver = &self.resolver;
        let snapshot_cache = &mut self.snapshot_cache;
        let semantic_cache_store = &mut self.semantic_cache_store;
        query.execute(resolver, snapshot_cache, semantic_cache_store)
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.next();
    }

    fn invalidate(&mut self, plan: InvalidationPlan) {
        self.snapshot_cache.invalidate(plan.clone());
        self.semantic_cache_store.invalidate(plan);
    }
}

impl Default for AnalysisDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::AnalysisDatabase;
    use crate::{DocumentUri, DocumentVersion};

    #[test]
    fn snapshot_reuses_semantic_cache_for_identical_state() {
        let uri = DocumentUri::new("untitled:syl/database-cache");
        let mut database = AnalysisDatabase::new();
        database.open_document(
            uri.clone(),
            r#"
package scratch;

module Cache(y: out Bit) {
    y := 1
}
"#
            .to_string(),
            DocumentVersion::new(1),
        );
        let first = database
            .snapshot()
            .expect("database should snapshot initial overlay");

        assert!(first.hwir().is_some());

        let second = database
            .snapshot()
            .expect("database should reuse the identical overlay snapshot");

        assert!(first.shares_semantic_cache_with(&second));
        assert!(
            second
                .hwir()
                .expect("shared semantic cache should remain available")
                .modules()
                .len()
                == 1
        );
    }

    #[test]
    fn document_scoped_invalidation_preserves_unrelated_snapshot_cache_entries() {
        let mut database = AnalysisDatabase::new();
        let base_uri = DocumentUri::new("untitled:syl/base");
        let overlay_uri = DocumentUri::new("untitled:syl/overlay");

        database.open_document(
            base_uri.clone(),
            r#"
package scratch;

module Base(y: out Bit) {
    y := 1
}
"#
            .to_string(),
            DocumentVersion::new(1),
        );
        let base = database
            .snapshot()
            .expect("database should snapshot the base overlay");
        assert!(base.hwir().is_some());

        database.open_document(
            overlay_uri.clone(),
            r#"
package scratch;

module Overlay(y: out Bit) {
    y := 1
}
"#
            .to_string(),
            DocumentVersion::new(1),
        );
        let overlay = database
            .snapshot()
            .expect("database should snapshot the overlayed state");
        assert!(overlay.hwir().is_some());
        assert!(!base.shares_semantic_cache_with(&overlay));

        let closed = database.close_document(&overlay_uri);
        assert!(closed.is_some());

        let restored = database
            .snapshot()
            .expect("closing an overlay should leave the base snapshot reusable");
        assert!(base.shares_semantic_cache_with(&restored));
        assert!(restored.hwir().is_some());
    }
}
