use crate::{CancellationToken, DocumentOrigin, DocumentUri, DocumentVersion, ProjectError};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::Arc,
};
use syl_elab::HardwareCompiler;
use syl_hw::ParametricHwDesign;
use syl_sema::{HirAnalysis, OpaqueSummaryTable, SemanticCompiler, TirAnalysis};
use syl_span::{Diagnostic, SourceId, SourceMap};
use syl_syntax::{AstFile, AstNodeIndex};

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
    pub(crate) semantic: Arc<SemanticCache>,
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
    pub fn new(parts: ResolvedSnapshot, semantic: Arc<SemanticCache>) -> Self {
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
            semantic,
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
        let package_files = self.package_files(uri);
        if package_files.is_empty() {
            return Ok(None);
        }
        Self::check_cancellation(token)?;

        let source_ids = package_files
            .iter()
            .map(|file| file.source_id())
            .collect::<BTreeSet<_>>();
        let parse = self
            .diagnostics()
            .iter()
            .filter(|diagnostic| source_ids.contains(&diagnostic.span.source))
            .cloned()
            .collect::<Vec<_>>();
        let ast_files = package_files
            .iter()
            .map(|file| file.ast().clone())
            .collect::<Vec<_>>();
        let hir_output = SemanticCompiler::new()
            .session(&ast_files)
            .resolve_hir_partial();
        let hir = hir_output.diagnostics().to_vec();
        Self::check_cancellation(token)?;

        let tir_output = hir_output.stage().check_tir_partial();
        let tir = tir_output.diagnostics().to_vec();
        let elaboration = if hir.is_empty() && tir.is_empty() {
            if let Some(tir_stage) = tir_output.partial_stage() {
                Self::check_cancellation(token)?;
                let opaque_summaries = self
                    .opaque_summaries()
                    .cloned()
                    .unwrap_or_else(OpaqueSummaryTable::new);
                HardwareCompiler::with_opaque_summaries(opaque_summaries)
                    .output_for_tir(tir_stage)
                    .diagnostics()
                    .to_vec()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
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
        self.semantic.diagnostics()
    }

    pub fn hir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        self.semantic.hir_diagnostics_with_token(token)
    }

    pub fn tir_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        self.semantic.tir_diagnostics_with_token(token)
    }

    pub fn elaboration_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&[Diagnostic], ProjectError> {
        self.semantic.elaboration_diagnostics_with_token(token)
    }

    pub fn hwir(&self) -> Option<&ParametricHwDesign> {
        if self.diagnostics.is_empty() {
            self.semantic.elaboration_output()?.hwir()
        } else {
            None
        }
    }

    pub fn hir_analysis(&self) -> &HirAnalysis {
        self.semantic.hir()
    }

    pub fn hir_analysis_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<&HirAnalysis, ProjectError> {
        self.semantic.hir_with_token(token)
    }

    pub fn tir_analysis(&self) -> Option<&TirAnalysis> {
        self.semantic.tir()
    }

    pub fn tir_analysis_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&TirAnalysis>, ProjectError> {
        self.semantic.tir_with_token(token)
    }

    pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        self.semantic.opaque_summaries()
    }

    pub fn opaque_summaries_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&OpaqueSummaryTable>, ProjectError> {
        self.semantic.opaque_summaries_with_token(token)
    }

    pub fn is_hir_cached(&self) -> bool {
        self.semantic.is_hir_cached()
    }

    pub fn is_tir_cached(&self) -> bool {
        self.semantic.is_tir_cached()
    }

    pub fn is_elaboration_cached(&self) -> bool {
        self.semantic.is_elaboration_cached()
    }

    pub fn shares_semantic_cache_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.semantic, &other.semantic)
    }

    pub fn hwir_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Option<&ParametricHwDesign>, ProjectError> {
        if !self.diagnostics.is_empty() {
            return Ok(None);
        }
        Ok(self
            .semantic
            .elaboration_output_with_token(token)?
            .and_then(|output| output.hwir()))
    }

    fn package_files(&self, uri: &DocumentUri) -> Vec<&AnalysisFile> {
        if let Some(package) = self.workspace.package_graph().package_for_uri(uri) {
            return self
                .files()
                .iter()
                .filter(|file| package.documents().contains(file.uri()))
                .collect();
        }
        self.file_by_uri(uri).into_iter().collect()
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
