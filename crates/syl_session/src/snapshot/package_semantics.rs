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
                let closure = closure_documents(workspace, package);
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

fn closure_documents(
    workspace: &WorkspaceSnapshot,
    package: &WorkspacePackage,
) -> Vec<DocumentUri> {
    let mut documents = BTreeSet::new();
    let mut visited = BTreeSet::new();
    collect_closure(workspace, package, &mut visited, &mut documents);
    documents.into_iter().collect()
}

fn collect_closure(
    workspace: &WorkspaceSnapshot,
    package: &WorkspacePackage,
    visited: &mut BTreeSet<Vec<String>>,
    documents: &mut BTreeSet<DocumentUri>,
) {
    if !visited.insert(package.path().to_vec()) {
        return;
    }
    documents.extend(package.documents().iter().cloned());
    for import in package.imports() {
        let Some(imported) = package_for_import(workspace, import.path()) else {
            continue;
        };
        collect_closure(workspace, imported, visited, documents);
    }
}

fn package_for_import<'a>(
    workspace: &'a WorkspaceSnapshot,
    path: &[String],
) -> Option<&'a WorkspacePackage> {
    let package_path = path.get(..path.len().checked_sub(1)?)?;
    workspace
        .package_graph()
        .packages()
        .iter()
        .find(|package| package.path() == package_path)
}
