use super::SemanticCache;
use crate::DocumentUri;
use std::{collections::BTreeMap, sync::Arc};

/// Probe interface for checking what analysis stages have cached results.
///
/// This is the public face of the semantic cache. Callers can check whether
/// HIR, TIR, or elaboration results are already cached without triggering a
/// re-analysis.
///
/// **Typical workflow:**
/// 1. Get a `PackageSemanticCacheProbe` from the snapshot.
/// 2. Check `is_hir_cached()` / `is_tir_cached()`.
/// 3. If cached, skip analysis; otherwise, run the stage.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct PackageSemanticCacheProbe {
    semantic: Arc<SemanticCache>,
}

impl PackageSemanticCacheProbe {
    pub(crate) fn new(semantic: Arc<SemanticCache>) -> Self {
        Self { semantic }
    }

    /// Returns `true` if HIR analysis results are already cached.
    pub fn is_hir_cached(&self) -> bool {
        self.semantic.is_hir_cached()
    }

    /// Returns `true` if TIR (type-inferred) analysis results are already cached.
    pub fn is_tir_cached(&self) -> bool {
        self.semantic.is_tir_cached()
    }

    pub fn is_elaboration_cached(&self) -> bool {
        self.semantic.is_elaboration_cached()
    }

    pub fn shares_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.semantic, &other.semantic)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct PackageSemanticShard {
    name: String,
    documents: Vec<DocumentUri>,
    semantic: Arc<SemanticCache>,
}

impl PackageSemanticShard {
    pub(crate) fn new(
        name: String,
        mut documents: Vec<DocumentUri>,
        semantic: Arc<SemanticCache>,
    ) -> Self {
        documents.sort();
        documents.dedup();
        Self {
            name,
            documents,
            semantic,
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn documents(&self) -> &[DocumentUri] {
        &self.documents
    }

    pub(crate) fn contains_document(&self, uri: &DocumentUri) -> bool {
        self.documents.contains(uri)
    }

    pub(crate) fn semantic(&self) -> &Arc<SemanticCache> {
        &self.semantic
    }

    pub(crate) fn probe(&self) -> PackageSemanticCacheProbe {
        PackageSemanticCacheProbe::new(Arc::clone(&self.semantic))
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PackageSemanticIndex {
    shards: BTreeMap<String, PackageSemanticShard>,
}

impl PackageSemanticIndex {
    pub(crate) fn new(shards: Vec<PackageSemanticShard>) -> Self {
        let shards = shards
            .into_iter()
            .map(|shard| (shard.name.clone(), shard))
            .collect();
        Self { shards }
    }

    pub(crate) fn entry(&self, package_name: &str) -> Option<&PackageSemanticShard> {
        self.shards.get(package_name)
    }

    pub(crate) fn entry_for_uri(&self, uri: &DocumentUri) -> Option<&PackageSemanticShard> {
        self.shards
            .values()
            .find(|shard| shard.contains_document(uri))
    }

    pub(crate) fn name_for_uri(&self, uri: &DocumentUri) -> Option<&str> {
        self.entry_for_uri(uri).map(PackageSemanticShard::name)
    }

    pub(crate) fn probe(&self, package_name: &str) -> Option<PackageSemanticCacheProbe> {
        self.entry(package_name).map(PackageSemanticShard::probe)
    }

    pub(crate) fn len(&self) -> usize {
        self.shards.len()
    }

    pub(crate) fn shards(&self) -> impl Iterator<Item = &PackageSemanticShard> {
        self.shards.values()
    }
}
