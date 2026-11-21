//! Package identity for a snapshot. Memos live in the Salsa database, not here.

use crate::DocumentUri;
use crate::snapshot::workspace::{WorkspacePackage, WorkspaceSnapshot};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug)]
pub(crate) struct PackageSemanticShard {
    name: String,
    documents: Vec<DocumentUri>,
    closure: Vec<DocumentUri>,
}

impl PackageSemanticShard {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn documents(&self) -> &[DocumentUri] {
        &self.documents
    }

    pub(crate) fn closure(&self) -> &[DocumentUri] {
        &self.closure
    }

    pub(crate) fn contains_document(&self, uri: &DocumentUri) -> bool {
        self.documents.iter().any(|document| document == uri)
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct PackageSemanticIndex {
    shards: BTreeMap<String, PackageSemanticShard>,
}

impl PackageSemanticIndex {
    pub(crate) fn from_workspace(workspace: &WorkspaceSnapshot) -> Self {
        let shards = workspace
            .package_graph()
            .packages()
            .iter()
            .map(|package| {
                let closure = PackageClosure::new(workspace).collect(package);
                let mut documents = package.documents().to_vec();
                documents.sort();
                documents.dedup();
                (
                    package.name().to_string(),
                    PackageSemanticShard {
                        name: package.name().to_string(),
                        documents,
                        closure,
                    },
                )
            })
            .collect();
        Self { shards }
    }

    pub(crate) fn entry_for_uri(&self, uri: &DocumentUri) -> Option<&PackageSemanticShard> {
        self.shards
            .values()
            .find(|shard| shard.contains_document(uri))
    }

    pub(crate) fn name_for_uri(&self, uri: &DocumentUri) -> Option<&str> {
        self.entry_for_uri(uri).map(PackageSemanticShard::name)
    }

    pub(crate) fn len(&self) -> usize {
        self.shards.len()
    }

    pub(crate) fn shards(&self) -> impl Iterator<Item = &PackageSemanticShard> {
        self.shards.values()
    }
}

/// Domain service for collecting a package's transitive source closure.
///
/// The visited set is an invariant of the operation: import cycles must be
/// harmless and each document must occur once in the resulting read model.
struct PackageClosure<'a> {
    workspace: &'a WorkspaceSnapshot,
    visited: BTreeSet<Vec<String>>,
    documents: BTreeSet<DocumentUri>,
}

impl<'a> PackageClosure<'a> {
    fn new(workspace: &'a WorkspaceSnapshot) -> Self {
        Self {
            workspace,
            visited: BTreeSet::new(),
            documents: BTreeSet::new(),
        }
    }

    fn collect(mut self, package: &WorkspacePackage) -> Vec<DocumentUri> {
        self.visit(package);
        self.documents.into_iter().collect()
    }

    fn visit(&mut self, package: &WorkspacePackage) {
        if !self.visited.insert(package.path().to_vec()) {
            return;
        }
        self.documents.extend(package.documents().iter().cloned());
        let imports = package.imports().to_vec();
        for import in imports {
            if let Some(imported) = self.package_for_import(import.path()).cloned() {
                self.visit(&imported);
            }
        }
    }

    fn package_for_import(&self, path: &[String]) -> Option<&WorkspacePackage> {
        let package_path = path.get(..path.len().checked_sub(1)?)?;
        self.workspace
            .package_graph()
            .packages()
            .iter()
            .find(|package| package.path() == package_path)
    }
}
