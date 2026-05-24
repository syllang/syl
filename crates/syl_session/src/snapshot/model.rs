use crate::{CancellationToken, DocumentOrigin, DocumentUri, DocumentVersion, ProjectError};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};
use syl_hw::ParametricHwDesign;
use syl_sema::{HirAnalysis, OpaqueSummaryTable, TirAnalysis};
use syl_span::{Diagnostic, SourceId, SourceMap};
use syl_syntax::{AstFile, AstNodeIndex};

use super::package_semantics::{PackageSemanticCacheProbe, PackageSemanticIndex};
use super::semantic_cache::SemanticCache;
use super::workspace::WorkspaceSnapshot;

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
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AnalysisFile {
    source_id: SourceId,
    path: Option<PathBuf>,
    uri: DocumentUri,
    version: DocumentVersion,
    origin: DocumentOrigin,
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
            ast,
            ast_node_index,
        } = input;
        Self {
            source_id,
            path,
            uri,
            version,
            origin,
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

    pub fn ast(&self) -> &AstFile {
        &self.ast
    }

    pub fn ast_node_index(&self) -> &AstNodeIndex {
        &self.ast_node_index
    }
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AnalysisSnapshot {
    pub(crate) source_map: SourceMap,
    pub(crate) files: Vec<AnalysisFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) workspace_semantic: Arc<SemanticCache>,
    pub(crate) package_semantics: PackageSemanticIndex,
    pub(crate) workspace: WorkspaceSnapshot,
}

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
    pub(crate) fn new(
        parts: ResolvedSnapshot,
        workspace_semantic: Arc<SemanticCache>,
        package_semantics: PackageSemanticIndex,
    ) -> Self {
        let ResolvedSnapshot {
            source_map,
            mut files,
            diagnostics,
            workspace,
        } = parts;
        files.sort_by(|lhs, rhs| lhs.uri.cmp(&rhs.uri));
        Self {
            source_map,
            files,
            diagnostics,
            workspace_semantic,
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

        let source_ids = self.package_source_ids(package.documents());
        let parse = self
            .diagnostics()
            .iter()
            .filter(|diagnostic| source_ids.contains(&diagnostic.span.source))
            .cloned()
            .collect::<Vec<_>>();
        let hir = package
            .semantic()
            .hir_diagnostics_with_token(token)?
            .to_vec();
        let tir = package
            .semantic()
            .tir_diagnostics_with_token(token)?
            .to_vec();
        let elaboration = package
            .semantic()
            .elaboration_diagnostics_with_token(token)?
            .to_vec();
        Ok(Some(PackageStageDiagnostics::new(
            parse,
            hir,
            tir,
            elaboration,
        )))
    }

    pub fn ast_files(&self) -> Vec<AstFile> {
        self.files.iter().map(|file| file.ast().clone()).collect()
    }

    pub fn semantic_diagnostics(&self) -> Vec<Diagnostic> {
        if self.package_semantics.len() <= 1 {
            return self.workspace_semantic.diagnostics();
        }
        let mut diagnostics = Vec::new();
        for package in self.package_semantics.shards() {
            diagnostics.extend(package.semantic().diagnostics());
        }
        diagnostics
    }

    pub fn hir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        self.workspace_semantic.hir_diagnostics_with_token(token)
    }

    pub fn tir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        self.workspace_semantic.tir_diagnostics_with_token(token)
    }

    pub fn elaboration_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        self.workspace_semantic
            .elaboration_diagnostics_with_token(token)
    }

    pub fn hwir(&self) -> Option<&ParametricHwDesign> {
        if self.diagnostics.is_empty() {
            self.workspace_semantic.elaboration_output()?.hwir()
        } else {
            None
        }
    }

    pub fn hir_analysis(&self) -> &HirAnalysis {
        self.workspace_semantic.hir()
    }

    pub fn hir_analysis_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&HirAnalysis, ProjectError> {
        self.workspace_semantic.hir_with_token(token)
    }

    pub fn tir_analysis(&self) -> Option<&TirAnalysis> {
        self.workspace_semantic.tir()
    }

    pub fn tir_analysis_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&TirAnalysis>, ProjectError> {
        self.workspace_semantic.tir_with_token(token)
    }

    pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        self.workspace_semantic.opaque_summaries()
    }

    pub fn opaque_summaries_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&OpaqueSummaryTable>, ProjectError> {
        self.workspace_semantic.opaque_summaries_with_token(token)
    }

    pub fn package_name_for_uri(&self, uri: &DocumentUri) -> Option<&str> {
        self.package_semantics.name_for_uri(uri)
    }

    pub fn package_semantic_cache(&self, package_name: &str) -> Option<PackageSemanticCacheProbe> {
        self.package_semantics.probe(package_name)
    }

    pub fn is_hir_cached(&self) -> bool {
        self.workspace_semantic.is_hir_cached()
    }

    pub fn is_tir_cached(&self) -> bool {
        self.workspace_semantic.is_tir_cached()
    }

    pub fn is_elaboration_cached(&self) -> bool {
        self.workspace_semantic.is_elaboration_cached()
    }

    pub fn shares_semantic_cache_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.workspace_semantic, &other.workspace_semantic)
    }

    pub fn hwir_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&ParametricHwDesign>, ProjectError> {
        if !self.diagnostics.is_empty() {
            return Ok(None);
        }
        Ok(self
            .workspace_semantic
            .elaboration_output_with_token(token)?
            .and_then(|output| output.hwir()))
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
}

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
