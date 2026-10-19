use crate::query::{
    FileSpec, Generation, LiveGuard, RevisionInput, SylDatabase, catch_query, ide_hir_query,
    ide_tir_query, make_unit, merged_opaque_query, package_diagnostics_query,
    workspace_diagnostics_query, workspace_elaboration_query, workspace_hir_query,
    workspace_tir_query,
};
use crate::{CancellationToken, DocumentOrigin, DocumentUri, DocumentVersion, ProjectError};
use std::{
    cell::RefCell,
    collections::BTreeSet,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};
use syl_hw::ParametricHwDesign;
use syl_sema::{HirAnalysis, OpaqueSummaryTable, TirAnalysis};
use syl_span::{Diagnostic, SourceId, SourceMap};
use syl_syntax::{AstFile, AstNodeIndex};

use super::package_semantics::{PackageSemanticIndex, PackageSemanticShard};
use super::retain::{self, Retained};
use super::workspace::WorkspaceSnapshot;

/// Parse/workspace resolution result before query inputs are published.
///
/// Built by the project resolver; [`AnalysisDatabase`](crate::AnalysisDatabase)
/// publishes it into the Salsa database and wraps an [`AnalysisSnapshot`].
///
/// # Main usage flow
///
/// ```text
///  ProjectResolver::snapshot
///         |
///         v
///  ResolvedSnapshot   (source_map, files, parse diagnostics, workspace)
///         |
///         v
///  AnalysisSnapshot::from_query(generation, inflight, live, resolved)
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct ResolvedSnapshot {
    pub(crate) source_map: SourceMap,
    pub(crate) files: Vec<AnalysisFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) workspace: WorkspaceSnapshot,
}

impl ResolvedSnapshot {
    pub fn new(
        source_map: SourceMap,
        files: Vec<AnalysisFile>,
        diagnostics: Vec<Diagnostic>,
        workspace: WorkspaceSnapshot,
    ) -> Self {
        Self {
            source_map,
            files,
            diagnostics,
            workspace,
        }
    }

    pub fn ast_files(&self) -> Vec<AstFile> {
        self.files.iter().map(|file| file.ast().clone()).collect()
    }

    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    pub fn files(&self) -> &[AnalysisFile] {
        &self.files
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn workspace(&self) -> &WorkspaceSnapshot {
        &self.workspace
    }

    pub(crate) fn file_specs(&self) -> Vec<FileSpec> {
        self.files
            .iter()
            .map(|file| {
                let text = self
                    .source_map
                    .file(file.source_id())
                    .expect("resolved file has a source map entry")
                    .text()
                    .to_string();
                FileSpec {
                    uri: file.uri_str().to_string(),
                    text,
                    source_id: file.source_id().get() as u32,
                    module_path: file.module_path().to_vec(),
                    version: file.version().get(),
                }
            })
            .collect()
    }
}

/// A parsed source file in the analysis snapshot with its AST and node index.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AnalysisFile {
    source_id: SourceId,
    path: Option<PathBuf>,
    uri: DocumentUri,
    version: DocumentVersion,
    origin: DocumentOrigin,
    module_path: Vec<String>,
    ast: AstFile,
    ast_node_index: AstNodeIndex,
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct AnalysisFileInput {
    pub(crate) source_id: SourceId,
    pub(crate) path: Option<PathBuf>,
    pub(crate) uri: DocumentUri,
    pub(crate) version: DocumentVersion,
    pub(crate) origin: DocumentOrigin,
    pub(crate) module_path: Vec<String>,
    pub(crate) ast: AstFile,
    pub(crate) ast_node_index: AstNodeIndex,
}

impl AnalysisFile {
    pub(crate) fn new(input: AnalysisFileInput) -> Self {
        let AnalysisFileInput {
            source_id,
            path,
            uri,
            version,
            origin,
            module_path,
            ast,
            ast_node_index,
        } = input;
        Self {
            source_id,
            path,
            uri,
            version,
            origin,
            module_path,
            ast,
            ast_node_index,
        }
    }

    pub fn source_id(&self) -> SourceId {
        self.source_id
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn uri_str(&self) -> &str {
        self.uri.as_str()
    }

    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    pub fn origin(&self) -> &DocumentOrigin {
        &self.origin
    }

    pub fn module_path(&self) -> &[String] {
        &self.module_path
    }

    pub fn ast(&self) -> &AstFile {
        &self.ast
    }

    pub fn ast_node_index(&self) -> &AstNodeIndex {
        &self.ast_node_index
    }
}

/// Immutable project analysis view at one revision — the session aggregate root.
///
/// Produced by [`AnalysisHost::load`](crate::AnalysisHost::load) /
/// [`snapshot`](crate::AnalysisHost::snapshot) (or the equivalent database
/// methods). Carries parsed files, workspace graph, and lazy semantic caches
/// (HIR / TIR / elaboration) shared across packages when possible.
///
/// # Main usage flow
///
/// ```text
///  AnalysisHost::load / snapshot
///              |
///              v
///    +----------------------+
///    |  AnalysisSnapshot    |
///    |  files + workspace     |
///    |  query revision      |
///    +----------+-----------+
///               |
///     +---------+-----------+--------------------+
///     |                     |                    |
///     v                     v                    v
///  Workspace / AST      Semantic stages      HWIR / emit
///  files()              hir_analysis()       hwir[ _with_token ]
///  file_by_uri()        tir_analysis()
///  source_map()         *_diagnostics_*
///  workspace()          package_stage_diagnostics_*
///  diagnostics()        opaque_summaries()
///
///  Editor-facing queries (definition / hover / completion / symbols)
///  live on syl_query::AnalysisQueries implemented for this snapshot.
/// ```
///
/// Token variants return [`ProjectError::Cancelled`] when cooperative
/// cancellation is observed mid-stage.
#[derive(Clone)]
#[non_exhaustive]
pub struct AnalysisSnapshot {
    // Host memo database. Later publishes advance it in place. `frozen_specs`
    // are this snapshot's inputs when that database has moved on.
    generation: Arc<Mutex<Generation>>,
    /// Shared with the host. Queries hold a read guard so publish waits.
    inflight: Arc<RwLock<()>>,
    frozen_specs: Vec<FileSpec>,
    frozen_opaque: OpaqueSummaryTable,
    owned: Arc<Mutex<Option<Arc<Mutex<Generation>>>>>,
    answers: RefCell<Retained>,
    live: LiveGuard,
    pub(crate) source_map: SourceMap,
    pub(crate) files: Vec<AnalysisFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) package_semantics: PackageSemanticIndex,
    pub(crate) workspace: WorkspaceSnapshot,
}

impl fmt::Debug for AnalysisSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("AnalysisSnapshot")
            .field("files", &self.files)
            .field("diagnostics", &self.diagnostics)
            .field("workspace", &self.workspace)
            .field("live", &self.live)
            .finish_non_exhaustive()
    }
}

/// Diagnostics grouped by compilation stage (parse, HIR, TIR, elaboration).
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct PackageStageDiagnostics {
    parse: Vec<Diagnostic>,
    hir: Vec<Diagnostic>,
    tir: Vec<Diagnostic>,
    elaboration: Vec<Diagnostic>,
}

impl PackageStageDiagnostics {
    fn new(
        parse: Vec<Diagnostic>,
        hir: Vec<Diagnostic>,
        tir: Vec<Diagnostic>,
        elaboration: Vec<Diagnostic>,
    ) -> Self {
        Self {
            parse,
            hir,
            tir,
            elaboration,
        }
    }

    pub fn parse(&self) -> &[Diagnostic] {
        &self.parse
    }

    pub fn hir(&self) -> &[Diagnostic] {
        &self.hir
    }

    pub fn tir(&self) -> &[Diagnostic] {
        &self.tir
    }

    pub fn elaboration(&self) -> &[Diagnostic] {
        &self.elaboration
    }
}

impl AnalysisSnapshot {
    pub(crate) fn from_query(
        generation: Arc<Mutex<Generation>>,
        inflight: Arc<RwLock<()>>,
        live: LiveGuard,
        parts: ResolvedSnapshot,
    ) -> Self {
        let ResolvedSnapshot {
            source_map,
            mut files,
            diagnostics,
            workspace,
        } = parts;
        files.sort_by(|lhs, rhs| lhs.uri().cmp(rhs.uri()));
        let package_semantics = PackageSemanticIndex::from_workspace(&workspace);
        let (frozen_specs, frozen_opaque) = {
            let current = Self::lock_generation(&generation);
            (
                current.file_specs(),
                current.revision.opaque(&current.db).clone(),
            )
        };
        Self {
            generation,
            inflight,
            frozen_specs,
            frozen_opaque,
            owned: Arc::new(Mutex::new(None)),
            answers: RefCell::new(Retained::default()),
            live,
            source_map,
            files,
            diagnostics,
            package_semantics,
            workspace,
        }
    }

    pub fn source_map(&self) -> &SourceMap {
        &self.source_map
    }

    pub fn files(&self) -> &[AnalysisFile] {
        &self.files
    }

    pub fn file_by_uri(&self, uri: &DocumentUri) -> Option<&AnalysisFile> {
        self.files.iter().find(|file| file.uri() == uri)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn workspace(&self) -> &WorkspaceSnapshot {
        &self.workspace
    }

    pub fn package_stage_diagnostics_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Option<PackageStageDiagnostics>, ProjectError> {
        let Some(package) = self.package_semantics.entry_for_uri(uri) else {
            return Ok(None);
        };
        Self::check_cancellation(token)?;
        let name = package.name().to_string();
        let local = Self::copied_uris(package.documents());
        let closure = Self::copied_uris(package.closure());
        let source_ids = self.package_source_ids(package.documents());
        let parse = self
            .diagnostics()
            .iter()
            .filter(|diagnostic| source_ids.contains(&diagnostic.span.source))
            .cloned()
            .collect::<Vec<_>>();
        Self::check_cancellation(token)?;
        let hir = catch_query(token, || {
            self.with_db(|db, revision| {
                let unit = make_unit(db, &name, local.clone(), closure.clone());
                Arc::clone(ide_hir_query(db, revision, unit))
            })
        })?;
        let hir_diagnostics = hir.diagnostics().to_vec();
        Self::check_cancellation(token)?;
        let tir = catch_query(token, || {
            self.with_db(|db, revision| {
                let unit = make_unit(db, &name, local.clone(), closure.clone());
                Arc::clone(ide_tir_query(db, revision, unit))
            })
        })?;
        let tir_diagnostics = tir.diagnostics().to_vec();
        Ok(Some(PackageStageDiagnostics::new(
            parse,
            hir_diagnostics,
            tir_diagnostics,
            Vec::new(),
        )))
    }

    pub fn ast_files(&self) -> Vec<AstFile> {
        self.files.iter().map(|file| file.ast().clone()).collect()
    }

    pub fn semantic_diagnostics(&self) -> Vec<Diagnostic> {
        if self.package_semantics.len() <= 1 {
            return self.with_db(|db, revision| workspace_diagnostics_query(db, revision).to_vec());
        }
        self.package_semantics
            .shards()
            .map(Self::shard_inputs)
            .collect::<Vec<_>>()
            .into_iter()
            .flat_map(|(name, local, closure)| {
                self.with_db(|db, revision| {
                    let unit = make_unit(db, &name, local, closure);
                    package_diagnostics_query(db, revision, unit).to_vec()
                })
            })
            .collect()
    }

    pub fn hir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        let output = catch_query(token, || {
            self.with_db(|db, revision| Arc::clone(workspace_hir_query(db, revision)))
        })?;
        Ok(retain::keep_hir(&self.answers, output).diagnostics())
    }

    pub fn tir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        let output = catch_query(token, || {
            self.with_db(|db, revision| Arc::clone(workspace_tir_query(db, revision)))
        })?;
        Ok(retain::keep_tir(&self.answers, output).diagnostics())
    }

    pub fn elaboration_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        if !self.diagnostics.is_empty() {
            return Ok(&[]);
        }
        let output = catch_query(token, || {
            self.with_db(|db, revision| workspace_elaboration_query(db, revision).clone())
        })?;
        Ok(match output {
            Some(value) => retain::keep_elaboration(&self.answers, value).diagnostics(),
            None => &[],
        })
    }

    pub fn hwir(&self) -> Option<&ParametricHwDesign> {
        if !self.diagnostics.is_empty() {
            return None;
        }
        let output =
            self.with_db(|db, revision| workspace_elaboration_query(db, revision).clone())?;
        retain::keep_elaboration(&self.answers, output).hwir()
    }

    pub fn hir_analysis(&self) -> &HirAnalysis {
        let output = self.with_db(|db, revision| Arc::clone(workspace_hir_query(db, revision)));
        retain::keep_hir(&self.answers, output).stage()
    }

    pub fn hir_analysis_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&HirAnalysis, ProjectError> {
        let output = catch_query(token, || {
            self.with_db(|db, revision| Arc::clone(workspace_hir_query(db, revision)))
        })?;
        Ok(retain::keep_hir(&self.answers, output).stage())
    }

    pub fn hir_analysis_for_uri_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Option<&HirAnalysis>, ProjectError> {
        let Some(output) = self.ide_arc(uri, token, |db, revision, unit| {
            Arc::clone(ide_hir_query(db, revision, unit))
        })?
        else {
            return Ok(None);
        };
        Ok(Some(retain::keep_hir(&self.answers, output).stage()))
    }

    pub fn tir_analysis(&self) -> Option<&TirAnalysis> {
        let output = self.with_db(|db, revision| Arc::clone(workspace_tir_query(db, revision)));
        retain::keep_tir(&self.answers, output).partial_stage()
    }

    pub fn tir_analysis_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&TirAnalysis>, ProjectError> {
        let output = catch_query(token, || {
            self.with_db(|db, revision| Arc::clone(workspace_tir_query(db, revision)))
        })?;
        Ok(retain::keep_tir(&self.answers, output).partial_stage())
    }

    pub fn tir_analysis_for_uri_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Option<&TirAnalysis>, ProjectError> {
        let Some(output) = self.ide_arc(uri, token, |db, revision, unit| {
            Arc::clone(ide_tir_query(db, revision, unit))
        })?
        else {
            return Ok(None);
        };
        Ok(retain::keep_tir(&self.answers, output).partial_stage())
    }

    pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        let output = self.with_db(|db, revision| merged_opaque_query(db, revision).clone())?;
        Some(retain::keep_opaque(&self.answers, output))
    }

    pub fn opaque_summaries_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&OpaqueSummaryTable>, ProjectError> {
        let output = catch_query(token, || {
            self.with_db(|db, revision| merged_opaque_query(db, revision).clone())
        })?;
        Ok(output.map(|value| retain::keep_opaque(&self.answers, value)))
    }

    pub fn package_name_for_uri(&self, uri: &DocumentUri) -> Option<&str> {
        self.package_semantics.name_for_uri(uri)
    }

    pub fn hwir_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&ParametricHwDesign>, ProjectError> {
        if !self.diagnostics.is_empty() {
            return Ok(None);
        }
        let output = catch_query(token, || {
            self.with_db(|db, revision| workspace_elaboration_query(db, revision).clone())
        })?;
        Ok(output.and_then(|value| retain::keep_elaboration(&self.answers, value).hwir()))
    }

    fn ide_arc<T>(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
        query: impl for<'db> Fn(&'db SylDatabase, RevisionInput, crate::query::UnitId<'db>) -> Arc<T>,
    ) -> Result<Option<Arc<T>>, ProjectError> {
        let Some(package) = self.package_semantics.entry_for_uri(uri) else {
            return Ok(None);
        };
        let (name, local, closure) = Self::shard_inputs(package);
        let value = catch_query(token, || {
            self.with_db(|db, revision| {
                let unit = make_unit(db, &name, local.clone(), closure.clone());
                query(db, revision, unit)
            })
        })?;
        Ok(Some(value))
    }

    fn with_db<T>(&self, body: impl FnOnce(&SylDatabase, RevisionInput) -> T) -> T {
        let _inflight = self
            .inflight
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let host = {
            let generation = Self::lock_generation(&self.generation);
            if generation.inputs_changed(&self.frozen_specs, &self.frozen_opaque) {
                None
            } else {
                Some((generation.db.clone(), generation.revision))
            }
        };
        if let Some((db, revision)) = host {
            return body(&db, revision);
        }
        drop(_inflight);
        let owned = self.frozen_generation();
        let (db, revision) = {
            let generation = Self::lock_generation(&owned);
            (generation.db.clone(), generation.revision)
        };
        body(&db, revision)
    }

    fn frozen_generation(&self) -> Arc<Mutex<Generation>> {
        let mut slot = self
            .owned
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = slot.as_ref() {
            return Arc::clone(existing);
        }
        let created = Arc::new(Mutex::new(Generation::from_specs(
            &self.frozen_specs,
            &self.frozen_opaque,
        )));
        *slot = Some(Arc::clone(&created));
        created
    }

    #[cfg(test)]
    pub(crate) fn probe_query_cycle(&self) {
        self.with_db(|db, revision| crate::query::probe_query_cycle(db, revision));
    }

    #[cfg(test)]
    pub(crate) fn execution_count(&self, needle: &str) -> usize {
        // The host database stays on this snapshot. Counts come from that
        // database even after a later publish moves its inputs.
        let generation = Self::lock_generation(&self.generation);
        generation.db.execution_count(needle)
    }

    #[cfg(test)]
    pub(crate) fn input_text(&self, uri: &str) -> Option<String> {
        self.with_db(|db, revision| crate::query::input_text(db, revision, uri))
    }

    fn package_source_ids(&self, documents: &[DocumentUri]) -> BTreeSet<SourceId> {
        self.files()
            .iter()
            .filter(|file| documents.contains(file.uri()))
            .map(AnalysisFile::source_id)
            .collect()
    }

    fn check_cancellation(token: &CancellationToken) -> Result<(), ProjectError> {
        if token.is_cancelled() {
            return Err(ProjectError::Cancelled);
        }
        Ok(())
    }
    fn lock_generation(generation: &Mutex<Generation>) -> std::sync::MutexGuard<'_, Generation> {
        generation
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn copied_uris(uris: &[DocumentUri]) -> Vec<String> {
        uris.iter().map(|uri| uri.as_str().to_string()).collect()
    }

    fn shard_inputs(package: &PackageSemanticShard) -> (String, Vec<String>, Vec<String>) {
        (
            package.name().to_string(),
            Self::copied_uris(package.documents()),
            Self::copied_uris(package.closure()),
        )
    }
}

/// Thin owned wrapper around an [`AnalysisSnapshot`] for tool-facing APIs.
///
/// # Main usage flow
///
/// ```text
///  AnalysisSnapshot
///         |
///         v
///  Project::new(snapshot)
///         |
///         +--> snapshot() / files() / diagnostics() / workspace()
///         +--> ProjectQueries (via syl_query) when needed
/// ```
#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct Project {
    snapshot: AnalysisSnapshot,
}

impl Project {
    pub fn new(snapshot: AnalysisSnapshot) -> Self {
        Self { snapshot }
    }

    pub fn snapshot(&self) -> &AnalysisSnapshot {
        &self.snapshot
    }

    pub fn source_map(&self) -> &SourceMap {
        self.snapshot().source_map()
    }

    pub fn files(&self) -> &[AnalysisFile] {
        self.snapshot().files()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.snapshot().diagnostics()
    }

    pub fn workspace(&self) -> &WorkspaceSnapshot {
        self.snapshot().workspace()
    }

    pub fn semantic_diagnostics(&self) -> Vec<Diagnostic> {
        self.snapshot().semantic_diagnostics()
    }

    pub fn ast_files(&self) -> Vec<AstFile> {
        self.snapshot().ast_files()
    }

    pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        self.snapshot().opaque_summaries()
    }
}
