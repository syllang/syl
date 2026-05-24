use crate::{
    AnalysisSnapshot, DocumentUri, Project, ProjectConfig, ProjectError, SourceDocument,
    collector::SylFileCollector, import_resolver::ImportResolver, snapshot::AnalysisFile,
    snapshot::AnalysisFileInput, snapshot::ResolvedSnapshot, snapshot::SemanticCache, vfs::FsVfs,
};
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use syl_span::{Diagnostic, SourceMap};
use syl_syntax::parser::SourceParser;
use syl_syntax::{AstFile, Item};

struct SnapshotBuildContext<'a> {
    source_map: &'a mut SourceMap,
    files: &'a mut Vec<AnalysisFile>,
    diagnostics: &'a mut Vec<Diagnostic>,
    queued: &'a mut VecDeque<PathBuf>,
    overlay_imports: &'a OverlayImportIndex,
}

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
        let mut input_paths = Vec::new();
        {
            let mut collector = SylFileCollector::new(&mut input_paths);
            for input in inputs {
                collector.collect(input)?;
            }
        }
        self.load_paths(input_paths)
    }

    pub fn load_paths(&self, paths: Vec<PathBuf>) -> Result<Project, ProjectError> {
        let resolved = self.snapshot(paths, &BTreeMap::new())?;
        let semantic = Arc::new(SemanticCache::new(resolved.ast_files()));
        Ok(Project::new(AnalysisSnapshot::new(resolved, semantic)))
    }

    pub(crate) fn snapshot(
        &self,
        paths: Vec<PathBuf>,
        overlays: &BTreeMap<DocumentUri, SourceDocument>,
    ) -> Result<ResolvedSnapshot, ProjectError> {
        let mut queued: VecDeque<PathBuf> = paths.into();
        let mut overlay_queued: VecDeque<DocumentUri> = overlays.keys().cloned().collect();
        let overlay_imports = OverlayImportIndex::new(overlays);
        let mut seen = BTreeSet::new();
        let mut source_map = SourceMap::new();
        let mut files = Vec::new();
        let mut diagnostics = Vec::new();
        while !queued.is_empty() || !overlay_queued.is_empty() {
            if let Some(path) = queued.pop_front() {
                let path = self.normalize_path(path)?;
                let uri = DocumentUri::from_file_path(&path);
                if !seen.insert(uri.clone()) {
                    continue;
                }
                let document = self.load_document(path, uri, overlays)?;
                let mut context = SnapshotBuildContext {
                    source_map: &mut source_map,
                    files: &mut files,
                    diagnostics: &mut diagnostics,
                    queued: &mut queued,
                    overlay_imports: &overlay_imports,
                };
                self.add_document(document, &mut context);
                continue;
            }

            let Some(uri) = overlay_queued.pop_front() else {
                continue;
            };
            if !seen.insert(uri.clone()) {
                continue;
            }
            if let Some(document) = overlays.get(&uri) {
                let mut context = SnapshotBuildContext {
                    source_map: &mut source_map,
                    files: &mut files,
                    diagnostics: &mut diagnostics,
                    queued: &mut queued,
                    overlay_imports: &overlay_imports,
                };
                self.add_document(document.clone(), &mut context);
            }
        }
        Ok(ResolvedSnapshot::new(source_map, files, diagnostics))
    }

    fn add_document(&self, document: SourceDocument, context: &mut SnapshotBuildContext<'_>) {
        let source_id = context
            .source_map
            .add_file(document.uri().as_str(), document.text());
        let parsed = SourceParser::new_in(document.text(), source_id).parse_file_partial();
        let ast_node_index = parsed.node_index().clone();
        let parsed_diagnostics = parsed.diagnostics;
        let ast = parsed.file;
        context.diagnostics.extend(parsed_diagnostics);
        self.queue_imports(document.uri(), &ast, context);
        context.files.push(AnalysisFile::new(AnalysisFileInput {
            source_id,
            path: document.path().map(Path::to_path_buf),
            uri: document.uri().clone(),
            version: document.version(),
            origin: document.origin().clone(),
            ast_node_index,
            ast,
        }));
    }

    fn load_document(
        &self,
        path: PathBuf,
        uri: DocumentUri,
        overlays: &BTreeMap<DocumentUri, SourceDocument>,
    ) -> Result<SourceDocument, ProjectError> {
        if let Some(document) = overlays.get(&uri) {
            return Ok(document.clone());
        }
        let source = self.import_resolver.vfs().read_to_string(&path)?;
        Ok(SourceDocument::from_disk(path, source))
    }

    fn queue_imports(
        &self,
        importer: &DocumentUri,
        file: &AstFile,
        context: &mut SnapshotBuildContext<'_>,
    ) {
        for item in &file.items {
            let Item::Use(item) = item else {
                continue;
            };
            match self.resolve_use(&item.path, context.overlay_imports) {
                Some(path) => context.queued.push_back(path),
                None => context.diagnostics.push(
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

    fn resolve_use(
        &self,
        parts: &[String],
        overlay_imports: &OverlayImportIndex,
    ) -> Option<PathBuf> {
        self.import_resolver
            .resolve_use(parts, |path| overlay_imports.contains(path))
    }

    fn normalize_path(&self, path: PathBuf) -> Result<PathBuf, ProjectError> {
        if self.import_resolver.vfs().exists(&path) {
            self.import_resolver.vfs().canonicalize(&path)
        } else {
            Ok(path)
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
struct OverlayImportIndex {
    paths: BTreeSet<PathBuf>,
}

impl OverlayImportIndex {
    fn new(overlays: &BTreeMap<DocumentUri, SourceDocument>) -> Self {
        let paths = overlays
            .values()
            .filter_map(|document| document.path().map(Path::to_path_buf))
            .collect();
        Self { paths }
    }

    fn contains(&self, path: &Path) -> bool {
        self.paths.contains(path)
    }
}
