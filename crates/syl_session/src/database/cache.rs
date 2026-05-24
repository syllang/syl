use crate::{
    AnalysisSnapshot, DocumentUri, DocumentVersion, SourceDocument, snapshot::AnalysisFile,
    snapshot::ResolvedSnapshot, snapshot::SemanticCache,
};
use std::{collections::BTreeMap, path::PathBuf, sync::Arc};
use syl_sema::OpaqueSummaryTable;
use syl_syntax::AstFile;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
/// Stable document identity for cache invalidation is the URI plus version.
pub(crate) struct DocumentKey {
    uri: DocumentUri,
    version: DocumentVersion,
}

impl DocumentKey {
    pub(crate) fn new(uri: DocumentUri, version: DocumentVersion) -> Self {
        Self { uri, version }
    }

    pub(crate) fn from_document(document: &SourceDocument) -> Self {
        Self::new(document.uri().clone(), document.version())
    }

    pub(crate) fn from_file(file: &AnalysisFile) -> Self {
        Self::new(file.uri().clone(), file.version())
    }

    pub(crate) fn uri(&self) -> &DocumentUri {
        &self.uri
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
struct DocumentFingerprint {
    key: DocumentKey,
    text_hash: u64,
}

impl DocumentFingerprint {
    fn new(key: DocumentKey, text_hash: u64) -> Self {
        Self { key, text_hash }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
/// Snapshot cache keys keep roots and sorted overlay fingerprints stable.
pub(crate) struct SnapshotKey {
    roots: Vec<PathBuf>,
    documents: Vec<DocumentFingerprint>,
}

impl SnapshotKey {
    pub(crate) fn from_inputs(inputs: &super::documents::DocumentInputs<'_>) -> Self {
        let mut documents = inputs
            .overlays()
            .iter()
            .map(|(uri, document)| {
                DocumentFingerprint::new(
                    DocumentKey::new(uri.clone(), document.version()),
                    hash_text(document.text()),
                )
            })
            .collect::<Vec<_>>();
        documents.sort();
        Self {
            roots: inputs.roots().to_vec(),
            documents,
        }
    }

    fn document_keys(&self) -> impl Iterator<Item = &DocumentKey> {
        self.documents.iter().map(|document| &document.key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
/// Semantic reuse is keyed by the resolved document set and per-file content hash.
pub(crate) struct SemanticSnapshotKey {
    documents: Vec<DocumentFingerprint>,
}

impl SemanticSnapshotKey {
    pub(crate) fn from_snapshot(snapshot: &ResolvedSnapshot) -> Self {
        let mut documents = snapshot
            .files()
            .iter()
            .map(|file| {
                let text = snapshot
                    .source_map()
                    .file(file.source_id())
                    .map(|source| source.text())
                    .unwrap_or_default();
                DocumentFingerprint::new(DocumentKey::from_file(file), hash_text(text))
            })
            .collect::<Vec<_>>();
        documents.sort();
        Self { documents }
    }

    fn document_keys(&self) -> impl Iterator<Item = &DocumentKey> {
        self.documents.iter().map(|document| &document.key)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum InvalidationPlan {
    ProjectGraphChanged,
    DocumentChanged(DocumentInvalidation),
}

impl InvalidationPlan {
    pub(crate) const fn project_graph_changed() -> Self {
        Self::ProjectGraphChanged
    }

    pub(crate) fn document_changed(change: DocumentInvalidation) -> Self {
        Self::DocumentChanged(change)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct DocumentInvalidation {
    key: DocumentKey,
    package_documents: Vec<Vec<DocumentUri>>,
}

impl DocumentInvalidation {
    pub(crate) fn new(key: DocumentKey, package_documents: Vec<Vec<DocumentUri>>) -> Self {
        Self {
            key,
            package_documents,
        }
    }

    fn key(&self) -> &DocumentKey {
        &self.key
    }

    fn package_documents(&self) -> &[Vec<DocumentUri>] {
        &self.package_documents
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) struct CachedSnapshot {
    key: SnapshotKey,
    snapshot: AnalysisSnapshot,
}

impl CachedSnapshot {
    pub(crate) fn new(key: SnapshotKey, snapshot: AnalysisSnapshot) -> Self {
        Self { key, snapshot }
    }

    pub(crate) fn matches_document(&self, key: &DocumentKey) -> bool {
        self.key.document_keys().any(|cached| cached == key)
    }

    pub(crate) fn matches_package_documents(&self, package_documents: &[DocumentUri]) -> bool {
        self.snapshot()
            .workspace()
            .package_graph()
            .packages()
            .iter()
            .any(|package| {
                package_documents
                    .iter()
                    .all(|document| package.documents().contains(document))
            })
    }

    pub(crate) fn snapshot(&self) -> &AnalysisSnapshot {
        &self.snapshot
    }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub(crate) struct SnapshotCache {
    cached: BTreeMap<SnapshotKey, CachedSnapshot>,
}

impl SnapshotCache {
    pub(crate) fn lookup(&self, key: &SnapshotKey) -> Option<AnalysisSnapshot> {
        self.cached.get(key).map(|cached| cached.snapshot().clone())
    }

    pub(crate) fn store(&mut self, snapshot: CachedSnapshot) -> AnalysisSnapshot {
        let result = snapshot.snapshot().clone();
        self.cached.insert(snapshot.key.clone(), snapshot);
        result
    }

    pub(crate) fn invalidate(&mut self, plan: InvalidationPlan) {
        match plan {
            InvalidationPlan::ProjectGraphChanged => self.cached.clear(),
            InvalidationPlan::DocumentChanged(change) => {
                self.cached.retain(|_, cached| {
                    !cached.matches_document(change.key())
                        && !change.package_documents().iter().any(|package_documents| {
                            cached.matches_package_documents(package_documents)
                        })
                });
            }
        }
    }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub(crate) struct SemanticCacheStore {
    cached: BTreeMap<SemanticSnapshotKey, CachedSemanticCache>,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
struct CachedSemanticCache {
    key: SemanticSnapshotKey,
    cache: Arc<SemanticCache>,
}

impl SemanticCacheStore {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn semantic_for_snapshot(
        &mut self,
        key: SemanticSnapshotKey,
        ast_files: Vec<AstFile>,
        opaque_summary_overlay: OpaqueSummaryTable,
    ) -> Arc<SemanticCache> {
        self.cached
            .entry(key.clone())
            .or_insert_with(|| CachedSemanticCache {
                key,
                cache: Arc::new(SemanticCache::new(ast_files, opaque_summary_overlay)),
            })
            .cache
            .clone()
    }

    pub(crate) fn invalidate(&mut self, plan: InvalidationPlan) {
        match plan {
            InvalidationPlan::ProjectGraphChanged => self.cached.clear(),
            InvalidationPlan::DocumentChanged(change) => {
                self.cached.retain(|_, cached| {
                    !cached.key.matches_document(change.key())
                        && !change.package_documents().iter().any(|package_documents| {
                            cached.key.matches_package_documents(package_documents)
                        })
                });
            }
        }
    }
}

impl SemanticSnapshotKey {
    fn matches_document(&self, key: &DocumentKey) -> bool {
        self.document_keys().any(|cached| cached == key)
    }

    fn matches_package_documents(&self, package_documents: &[DocumentUri]) -> bool {
        package_documents
            .iter()
            .all(|document| self.document_keys().any(|cached| cached.uri == *document))
    }
}

fn hash_text(text: &str) -> u64 {
    const FNV_OFFSET_BASIS: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET_BASIS;
    for byte in text.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::hash_text;

    #[test]
    fn hash_text_is_stable() {
        assert_eq!(hash_text(""), 0xcbf29ce484222325);
        assert_eq!(hash_text("syl"), hash_text("syl"));
        assert_ne!(hash_text("syl"), hash_text("syl!"));
    }
}
