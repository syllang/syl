use crate::{AnalysisFile, DocumentOrigin, DocumentUri, DocumentVersion};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use syl_syntax::Item;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkspaceSnapshot {
    roots: Vec<PathBuf>,
    source_database: SourceDatabase,
    package_graph: PackageGraph,
}

impl WorkspaceSnapshot {
    pub(crate) fn collect(roots: Vec<PathBuf>, files: &[AnalysisFile]) -> Self {
        WorkspaceSnapshotBuilder::new(roots, files).build()
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn source_database(&self) -> &SourceDatabase {
        &self.source_database
    }

    pub fn package_graph(&self) -> &PackageGraph {
        &self.package_graph
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceDatabase {
    documents: Vec<SourceDatabaseDocument>,
}

impl SourceDatabase {
    fn new(documents: Vec<SourceDatabaseDocument>) -> Self {
        Self { documents }
    }

    pub fn documents(&self) -> &[SourceDatabaseDocument] {
        &self.documents
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceDatabaseDocument {
    uri: DocumentUri,
    path: Option<PathBuf>,
    version: DocumentVersion,
    origin: DocumentOrigin,
}

impl SourceDatabaseDocument {
    fn from_file(file: &AnalysisFile) -> Self {
        Self {
            uri: file.uri().clone(),
            path: file.path().map(Path::to_path_buf),
            version: file.version(),
            origin: file.origin().clone(),
        }
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    pub fn origin(&self) -> &DocumentOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageGraph {
    packages: Vec<WorkspacePackage>,
}

impl PackageGraph {
    fn new(mut packages: Vec<WorkspacePackage>) -> Self {
        packages.sort_by(|lhs, rhs| lhs.name.cmp(&rhs.name).then(lhs.path.cmp(&rhs.path)));
        Self { packages }
    }

    pub fn packages(&self) -> &[WorkspacePackage] {
        &self.packages
    }

    pub fn package_for_uri(&self, uri: &DocumentUri) -> Option<&WorkspacePackage> {
        self.packages
            .iter()
            .find(|package| package.documents.iter().any(|document| document == uri))
    }

    pub(crate) fn packages_for_uri(&self, uri: &DocumentUri) -> Vec<&WorkspacePackage> {
        self.packages
            .iter()
            .filter(|package| package.documents.iter().any(|document| document == uri))
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkspacePackage {
    name: String,
    path: Vec<String>,
    documents: Vec<DocumentUri>,
    imports: Vec<PackageImport>,
}

impl WorkspacePackage {
    fn new(
        name: String,
        path: Vec<String>,
        mut documents: Vec<DocumentUri>,
        mut imports: Vec<PackageImport>,
    ) -> Self {
        documents.sort();
        documents.dedup();
        imports.sort_by(|lhs, rhs| lhs.path.cmp(&rhs.path));
        imports.dedup();
        Self {
            name,
            path,
            documents,
            imports,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn path(&self) -> &[String] {
        &self.path
    }

    pub fn documents(&self) -> &[DocumentUri] {
        &self.documents
    }

    pub fn imports(&self) -> &[PackageImport] {
        &self.imports
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct PackageImport {
    path: Vec<String>,
}

impl PackageImport {
    fn new(path: Vec<String>) -> Self {
        Self { path }
    }

    pub fn path(&self) -> &[String] {
        &self.path
    }
}

#[derive(Debug)]
struct WorkspaceSnapshotBuilder<'a> {
    roots: Vec<PathBuf>,
    files: &'a [AnalysisFile],
}

impl<'a> WorkspaceSnapshotBuilder<'a> {
    fn new(roots: Vec<PathBuf>, files: &'a [AnalysisFile]) -> Self {
        Self { roots, files }
    }

    fn build(self) -> WorkspaceSnapshot {
        let source_database = SourceDatabase::new(
            self.files
                .iter()
                .map(SourceDatabaseDocument::from_file)
                .collect(),
        );
        let package_graph = PackageGraph::new(self.collect_packages());
        WorkspaceSnapshot {
            roots: self.roots,
            source_database,
            package_graph,
        }
    }

    fn collect_packages(&self) -> Vec<WorkspacePackage> {
        let mut packages = BTreeMap::<PackageKey, PackageAccumulator>::new();
        for file in self.files {
            let path = file.module_path().to_vec();
            let name = self.package_name(file, &path);
            let key = PackageKey::new(name, path);
            let package = packages.entry(key).or_default();
            package.documents.push(file.uri().clone());
            package
                .imports
                .extend(file.ast().items.iter().filter_map(|item| match item {
                    Item::Use(item) => Some(PackageImport::new(item.path.clone())),
                    _ => None,
                }));
        }

        packages
            .into_iter()
            .map(|(key, package)| {
                WorkspacePackage::new(key.name, key.path, package.documents, package.imports)
            })
            .collect()
    }

    fn package_name(&self, file: &AnalysisFile, path: &[String]) -> String {
        if path.is_empty() {
            file.uri().to_string()
        } else {
            path.join(".")
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct PackageKey {
    name: String,
    path: Vec<String>,
}

impl PackageKey {
    fn new(name: String, path: Vec<String>) -> Self {
        Self { name, path }
    }
}

#[derive(Debug, Default)]
struct PackageAccumulator {
    documents: Vec<DocumentUri>,
    imports: Vec<PackageImport>,
}
