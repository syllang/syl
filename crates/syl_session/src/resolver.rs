use crate::{
    AnalysisSnapshot, CancellationToken, DocumentUri, Project, ProjectConfig, ProjectError,
    SourceDocument, collector::SylFileCollector, import_resolver::ImportResolver,
    query::QueryStore, snapshot::AnalysisFile, snapshot::AnalysisFileInput,
    snapshot::ResolvedSnapshot, snapshot::WorkspaceSnapshot, vfs::FsVfs,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;
use syl_sema::OpaqueSummaryTable;
use syl_span::{Diagnostic, SourceId, SourceMap, Span};
use syl_syntax::parser::SourceParser;
use syl_syntax::{AstFile, Item};

#[cfg(test)]
#[path = "resolver_test.rs"]
mod resolver_test;

#[derive(Debug)]
#[non_exhaustive]
pub struct ProjectResolver<V = FsVfs>
where
    V: crate::vfs::Vfs,
{
    import_resolver: ImportResolver<V>,
}

impl ProjectResolver<FsVfs> {
    pub fn new() -> Self {
        Self::with_config(ProjectConfig::new())
    }

    pub fn with_config(config: ProjectConfig) -> Self {
        Self::with_vfs(config, FsVfs)
    }
}

impl Default for ProjectResolver<FsVfs> {
    fn default() -> Self {
        Self::new()
    }
}

impl<V> ProjectResolver<V>
where
    V: crate::vfs::Vfs,
{
    pub fn with_vfs(config: ProjectConfig, vfs: V) -> Self {
        Self {
            import_resolver: ImportResolver::with_vfs(config, vfs),
        }
    }

    pub fn config(&self) -> &ProjectConfig {
        self.import_resolver.config()
    }

    pub fn load(&self, inputs: &[PathBuf]) -> Result<Project, ProjectError> {
        self.load_with_token(inputs, &CancellationToken::new())
    }

    pub fn load_with_token(
        &self,
        inputs: &[PathBuf],
        token: &CancellationToken,
    ) -> Result<Project, ProjectError> {
        let mut input_paths = Vec::new();
        {
            let mut collector = SylFileCollector::new(&mut input_paths);
            for input in inputs {
                collector.collect(input)?;
            }
        }
        self.load_paths_with_token(input_paths, token)
    }

    pub fn load_paths(&self, paths: Vec<PathBuf>) -> Result<Project, ProjectError> {
        self.load_paths_with_token(paths, &CancellationToken::new())
    }

    pub fn load_paths_with_token(
        &self,
        paths: Vec<PathBuf>,
        token: &CancellationToken,
    ) -> Result<Project, ProjectError> {
        let resolved = self.snapshot(paths, &BTreeMap::new(), token)?;
        let mut query = QueryStore::new();
        let opaque = OpaqueSummaryTable::new();
        query.publish(&resolved.file_specs(), &opaque);
        let live = Arc::new(AtomicUsize::new(0));
        Ok(Project::new(AnalysisSnapshot::from_query(
            query.generation(),
            query.inflight(),
            crate::query::LiveGuard::enter(&live),
            resolved,
        )))
    }

    pub(crate) fn snapshot(
        &self,
        paths: Vec<PathBuf>,
        overlays: &BTreeMap<DocumentUri, SourceDocument>,
        token: &CancellationToken,
    ) -> Result<ResolvedSnapshot, ProjectError> {
        SnapshotAssembler::new(self, paths, overlays).build(token)
    }
}

/// Application service that turns project inputs into one immutable snapshot.
///
/// The resolver supplies policy and infrastructure. The assembler owns the
/// short-lived build state (queues, de-duplication, source map and diagnostics)
/// so the resolver itself remains a reusable service rather than a mutable
/// procedure with a large parameter list.
struct SnapshotAssembler<'resolver, 'inputs, V>
where
    V: crate::vfs::Vfs,
{
    resolver: &'resolver ProjectResolver<V>,
    roots: Vec<PathBuf>,
    overlays: &'inputs BTreeMap<DocumentUri, SourceDocument>,
    overlay_imports: OverlayImportCatalog,
    queued_paths: VecDeque<PathBuf>,
    queued_overlays: VecDeque<DocumentUri>,
    seen: BTreeSet<DocumentUri>,
    source_map: SourceMap,
    files: Vec<AnalysisFile>,
    diagnostics: Vec<Diagnostic>,
}

impl<'resolver, 'inputs, V> SnapshotAssembler<'resolver, 'inputs, V>
where
    V: crate::vfs::Vfs,
{
    fn new(
        resolver: &'resolver ProjectResolver<V>,
        roots: Vec<PathBuf>,
        overlays: &'inputs BTreeMap<DocumentUri, SourceDocument>,
    ) -> Self {
        let queued_overlays = overlays.keys().cloned().collect();
        Self {
            resolver,
            queued_paths: roots.clone().into(),
            roots,
            overlays,
            overlay_imports: OverlayImportCatalog::from_documents(overlays),
            queued_overlays,
            seen: BTreeSet::new(),
            source_map: SourceMap::new(),
            files: Vec::new(),
            diagnostics: Vec::new(),
        }
    }

    fn build(mut self, token: &CancellationToken) -> Result<ResolvedSnapshot, ProjectError> {
        Self::ensure_active(token)?;
        while !self.queued_paths.is_empty() || !self.queued_overlays.is_empty() {
            Self::ensure_active(token)?;
            if let Some(path) = self.queued_paths.pop_front() {
                self.visit_path(path)?;
            } else if let Some(uri) = self.queued_overlays.pop_front() {
                self.visit_overlay(uri)?;
            }
        }
        let workspace = WorkspaceSnapshot::collect(self.roots, &self.files);
        Ok(ResolvedSnapshot::new(
            self.source_map,
            self.files,
            self.diagnostics,
            workspace,
        ))
    }

    fn visit_path(&mut self, path: PathBuf) -> Result<(), ProjectError> {
        let path = self.normalize_path(path)?;
        let uri = DocumentUri::from_file_path(&path);
        if !self.seen.insert(uri.clone()) {
            return Ok(());
        }
        let document = self.load_document(path, uri)?;
        self.add_document(document);
        Ok(())
    }

    fn visit_overlay(&mut self, uri: DocumentUri) -> Result<(), ProjectError> {
        if !self.seen.insert(uri.clone()) {
            return Ok(());
        }
        if let Some(document) = self.overlays.get(&uri) {
            self.add_document(document.clone());
        }
        Ok(())
    }

    fn add_document(&mut self, document: SourceDocument) {
        let source_id = self
            .source_map
            .add_file(document.uri().as_str(), document.text());
        let parsed = SourceParser::new_in(document.text(), source_id).parse_file_partial();
        let ast_node_index = parsed.node_index().clone();
        let ast = parsed.file;
        self.diagnostics.extend(parsed.diagnostics);
        let (module_path, mut module_diagnostics) =
            ModulePath::for_document(self.resolver, &document, source_id);
        self.diagnostics.append(&mut module_diagnostics);
        self.queue_imports(document.uri(), &ast);
        self.files.push(AnalysisFile::new(AnalysisFileInput {
            source_id,
            path: document.path().map(Path::to_path_buf),
            uri: document.uri().clone(),
            version: document.version(),
            origin: document.origin().clone(),
            module_path: module_path.into_parts(),
            ast_node_index,
            ast,
        }));
    }

    fn load_document(
        &self,
        path: PathBuf,
        uri: DocumentUri,
    ) -> Result<SourceDocument, ProjectError> {
        if let Some(document) = self.overlays.get(&uri) {
            return Ok(document.clone());
        }
        let source = self.resolver.import_resolver.vfs().read_to_string(&path)?;
        Ok(SourceDocument::from_disk(path, source))
    }

    fn queue_imports(&mut self, importer: &DocumentUri, file: &AstFile) {
        for item in &file.items {
            let Item::Use(item) = item else { continue };
            match self
                .resolver
                .import_resolver
                .resolve_use(&item.path, |path| self.overlay_imports.contains(path))
            {
                Some(path) => self.queued_paths.push_back(path),
                None => self.diagnostics.push(
                    Diagnostic::new(
                        item.span,
                        format!(
                            "failed to resolve import {} from {}",
                            item.path.join("."),
                            importer
                        ),
                    )
                    .with_code("E_IMPORT_RESOLVE")
                    .with_source("syl_session"),
                ),
            }
        }
    }

    fn normalize_path(&self, path: PathBuf) -> Result<PathBuf, ProjectError> {
        if self.resolver.import_resolver.vfs().exists(&path) {
            self.resolver.import_resolver.vfs().canonicalize(&path)
        } else {
            Ok(path)
        }
    }

    fn ensure_active(token: &CancellationToken) -> Result<(), ProjectError> {
        if token.is_cancelled() {
            Err(ProjectError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// Value object for a module identity.
///
/// Module paths are validated and constructed together so callers cannot
/// accidentally use a path without applying the session's naming rules.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ModulePath {
    parts: Vec<String>,
}

impl ModulePath {
    fn for_document<V: crate::vfs::Vfs>(
        resolver: &ProjectResolver<V>,
        document: &SourceDocument,
        source_id: SourceId,
    ) -> (Self, Vec<Diagnostic>) {
        let path = document
            .path()
            .and_then(|path| Self::for_path(resolver, path))
            .unwrap_or_else(|| Self::fallback(document));
        let diagnostics = path
            .parts
            .iter()
            .filter(|part| !Self::is_valid_segment(part))
            .map(|part| {
                Diagnostic::new(
                    Span::new_in(source_id, 0, 0),
                    format!("invalid module path segment `{part}`"),
                )
                .with_code("E_INVALID_MODULE_PATH")
                .with_source("syl_session")
            })
            .collect();
        (path, diagnostics)
    }

    fn into_parts(self) -> Vec<String> {
        self.parts
    }

    fn for_path<V: crate::vfs::Vfs>(resolver: &ProjectResolver<V>, path: &Path) -> Option<Self> {
        for root in resolver.import_resolver.config().std_roots() {
            if let Some(mut parts) = Self::relative_to_root(resolver, path, root) {
                parts.insert(0, "std".to_string());
                return Some(Self { parts });
            }
        }
        for root in resolver.import_resolver.config().workspace_roots() {
            if let Some(parts) = Self::relative_to_root(resolver, path, root) {
                return Some(Self { parts });
            }
        }
        for root in resolver.import_resolver.config().package_roots() {
            if let Some(parts) = Self::relative_to_root(resolver, path, root) {
                return Some(Self { parts });
            }
        }
        path.file_stem()
            .and_then(|stem| stem.to_str())
            .map(|stem| Self {
                parts: vec![stem.to_string()],
            })
    }

    fn fallback(document: &SourceDocument) -> Self {
        let parts = document
            .uri()
            .as_str()
            .rsplit(['/', ':'])
            .find(|segment| !segment.is_empty())
            .map(|segment| vec![segment.to_string()])
            .unwrap_or_else(|| vec!["scratch".to_string()]);
        Self { parts }
    }

    fn relative_to_root<V: crate::vfs::Vfs>(
        resolver: &ProjectResolver<V>,
        path: &Path,
        root: &Path,
    ) -> Option<Vec<String>> {
        Self::relative_to(path, root).or_else(|| {
            resolver
                .import_resolver
                .vfs()
                .canonicalize(root)
                .ok()
                .and_then(|root| Self::relative_to(path, &root))
        })
    }

    fn relative_to(path: &Path, root: &Path) -> Option<Vec<String>> {
        let relative = path.strip_prefix(root).ok()?;
        let parts = relative
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .filter(|segment| *segment != ".")
            .map(|segment| segment.strip_suffix(".syl").unwrap_or(segment).to_string())
            .collect::<Vec<_>>();
        (!parts.is_empty()).then_some(parts)
    }

    fn is_valid_segment(segment: &str) -> bool {
        let mut chars = segment.chars();
        let Some(first) = chars.next() else {
            return false;
        };
        (first == '_' || first.is_ascii_alphabetic())
            && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
    }
}

#[derive(Debug)]
#[non_exhaustive]
struct OverlayImportCatalog {
    paths: BTreeSet<PathBuf>,
}

impl OverlayImportCatalog {
    fn from_documents(overlays: &BTreeMap<DocumentUri, SourceDocument>) -> Self {
        Self {
            paths: overlays
                .values()
                .filter_map(|document| document.path().map(Path::to_path_buf))
                .collect(),
        }
    }

    fn contains(&self, path: &Path) -> bool {
        self.paths.contains(path)
    }
}
