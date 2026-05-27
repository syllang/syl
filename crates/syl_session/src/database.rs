mod cache;
mod documents;
mod revision;

use crate::{
    AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig, ProjectError,
    ProjectResolver, SourceDocument, WorkspaceSnapshot, collector::SylFileCollector,
    snapshot::SemanticCache,
};
use cache::{
    CachedSnapshot, DocumentInvalidation, DocumentKey, InvalidationPlan, SemanticCacheStore,
    SnapshotCache, SnapshotKey,
};
use documents::{DocumentInputs, DocumentStore};
use std::{path::PathBuf, sync::Arc};
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable};

pub use revision::DatabaseRevision;

#[derive(Debug)]
#[non_exhaustive]
pub struct AnalysisDatabase {
    resolver: ProjectResolver,
    documents: DocumentStore,
    opaque_summaries: OpaqueSummaryTable,
    snapshot_cache: SnapshotCache,
    semantic_cache_store: SemanticCacheStore,
    revision: DatabaseRevision,
    last_workspace: Option<WorkspaceSnapshot>,
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
        context: SnapshotQueryExecution<'_>,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        if let Some(snapshot) = context.snapshot_cache.lookup(&self.key) {
            return Ok(snapshot);
        }

        let (roots, overlays) = self.inputs.into_resolver_inputs();
        let resolved = context.resolver.snapshot(roots, overlays, context.token)?;
        let workspace_semantic = Arc::new(SemanticCache::new_sources(
            resolved
                .files()
                .iter()
                .map(|file| {
                    crate::snapshot::SemanticCacheSource::new(
                        file.module_path().to_vec(),
                        file.ast().clone(),
                    )
                })
                .collect(),
            context.opaque_summaries.clone(),
        ));
        let package_semantics = context
            .semantic_cache_store
            .package_shards_for_snapshot(&resolved, context.opaque_summaries);
        let snapshot = AnalysisSnapshot::new(resolved, workspace_semantic, package_semantics);
        let cached = CachedSnapshot::new(self.key.clone(), snapshot);
        Ok(context.snapshot_cache.store(cached))
    }
}

struct SnapshotQueryExecution<'a> {
    resolver: &'a ProjectResolver,
    opaque_summaries: &'a OpaqueSummaryTable,
    snapshot_cache: &'a mut SnapshotCache,
    semantic_cache_store: &'a mut SemanticCacheStore,
    token: &'a CancellationToken,
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
            snapshot_cache: SnapshotCache::default(),
            semantic_cache_store: SemanticCacheStore::new(),
            revision: DatabaseRevision::initial(),
            last_workspace: None,
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
        self.last_workspace = None;
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
            let plan = self.document_invalidation(previous);
            self.invalidate(InvalidationPlan::document_changed(plan));
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
            let plan = self.document_invalidation(previous);
            self.invalidate(InvalidationPlan::document_changed(plan));
        } else {
            self.advance_revision();
        }
        Ok(version)
    }

    pub fn close_document(&mut self, uri: &DocumentUri) -> Option<SourceDocument> {
        let removed = self.documents.close_document(uri);
        if let Some(document) = removed.as_ref() {
            self.advance_revision();
            let plan = self.document_invalidation(DocumentKey::from_document(document));
            self.invalidate(InvalidationPlan::document_changed(plan));
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
        self.invalidate(InvalidationPlan::project_graph_changed());
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
        let query = SnapshotQuery::new(self.documents.snapshot_inputs());
        let snapshot = query.execute(SnapshotQueryExecution {
            resolver: &self.resolver,
            opaque_summaries: &self.opaque_summaries,
            snapshot_cache: &mut self.snapshot_cache,
            semantic_cache_store: &mut self.semantic_cache_store,
            token,
        })?;
        self.last_workspace = Some(snapshot.workspace().clone());
        Ok(snapshot)
    }

    fn advance_revision(&mut self) {
        self.revision = self.revision.next();
    }

    fn invalidate(&mut self, plan: InvalidationPlan) {
        self.snapshot_cache.invalidate(plan.clone());
        self.semantic_cache_store.invalidate(plan);
    }

    fn document_invalidation(&self, key: DocumentKey) -> DocumentInvalidation {
        let package_documents = self
            .last_workspace
            .as_ref()
            .map(|workspace| {
                workspace
                    .package_graph()
                    .packages_for_uri(key.uri())
                    .into_iter()
                    .map(|package| package.documents().to_vec())
                    .collect()
            })
            .unwrap_or_default();
        DocumentInvalidation::new(key, package_documents)
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
    use crate::{CancellationToken, DocumentUri, DocumentVersion, ProjectError};

    #[test]
    fn snapshot_reuses_semantic_cache_for_identical_state() {
        let uri = DocumentUri::new("untitled:syl/cache");
        let mut database = AnalysisDatabase::new();
        database.open_document(
            uri.clone(),
            r#"
cell Cache(y: out Bit) {
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
cell Base(y: out Bit) {
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
cell Overlay(y: out Bit) {
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

    #[test]
    fn package_semantic_shards_reuse_unmodified_packages_after_package_edit() {
        let first_uri = DocumentUri::new("untitled:syl/first");
        let second_uri = DocumentUri::new("untitled:syl/second");
        let mut database = AnalysisDatabase::new();
        database.open_document(
            first_uri,
            "cell First(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        database.open_document(
            second_uri.clone(),
            "cell Second(y: out Bit) { y := 0 }\n".to_string(),
            DocumentVersion::new(1),
        );

        let baseline = database
            .snapshot()
            .expect("package shard baseline fixture must snapshot");
        let baseline_first = baseline
            .package_semantic_cache("first")
            .expect("first package shard must exist");
        let baseline_second = baseline
            .package_semantic_cache("second")
            .expect("second package shard must exist");

        database
            .update_document_at_version(
                &second_uri,
                "cell Second(y: out Bit) { y := 1 }\n".to_string(),
                DocumentVersion::new(2),
            )
            .expect("package shard edit must update the second package");
        let updated = database
            .snapshot()
            .expect("package shard update fixture must snapshot");
        let updated_first = updated
            .package_semantic_cache("first")
            .expect("first package shard must still exist");
        let updated_second = updated
            .package_semantic_cache("second")
            .expect("second package shard must still exist");

        assert!(baseline_first.shares_with(&updated_first));
        assert!(!baseline_second.shares_with(&updated_second));
    }

    #[test]
    fn workspace_snapshot_tracks_source_database_and_package_graph() {
        let first_uri = DocumentUri::new("untitled:syl/first");
        let second_uri = DocumentUri::new("untitled:syl/second");
        let mut database = AnalysisDatabase::new();
        database.open_document(
            first_uri.clone(),
            "cell First(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        database.open_document(
            second_uri.clone(),
            "cell Second(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );

        let snapshot = database
            .snapshot()
            .expect("workspace snapshot fixture must build");
        let workspace = snapshot.workspace();

        assert_eq!(workspace.source_database().documents().len(), 2);
        assert_eq!(workspace.package_graph().packages().len(), 2);
        assert!(workspace.package_graph().packages().iter().any(|package| {
            package.name() == "first" && package.documents().contains(&first_uri)
        }));
        assert!(workspace.package_graph().packages().iter().any(|package| {
            package.name() == "second" && package.documents().contains(&second_uri)
        }));
    }

    #[test]
    fn cancelled_snapshot_stops_before_resolution() {
        let uri = DocumentUri::new("untitled:syl/app");
        let mut database = AnalysisDatabase::new();
        database.open_document(
            uri,
            "cell Top(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        let token = CancellationToken::new();
        token.cancel();

        let err = database
            .snapshot_with_token(&token)
            .expect_err("cancelled snapshot should stop before rebuilding");

        assert!(matches!(err, ProjectError::Cancelled));
    }
}
