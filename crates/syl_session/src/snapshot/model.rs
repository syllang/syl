use crate::{DocumentOrigin, DocumentUri, DocumentVersion};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use syl_hw::ParametricHwDesign;
use syl_sema::{HirAnalysis, TirAnalysis};
use syl_span::{Diagnostic, SourceId, SourceMap};
use syl_syntax::AstFile;

use super::semantic_cache::SemanticCache;

#[derive(Debug)]
#[non_exhaustive]
pub struct ResolvedSnapshot {
    pub(crate) source_map: SourceMap,
    pub(crate) files: Vec<AnalysisFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
}

impl ResolvedSnapshot {
    pub fn new(
        source_map: SourceMap,
        files: Vec<AnalysisFile>,
        diagnostics: Vec<Diagnostic>,
    ) -> Self {
        Self {
            source_map,
            files,
            diagnostics,
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
        } = input;
        Self {
            source_id,
            path,
            uri,
            version,
            origin,
            ast,
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
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub struct AnalysisSnapshot {
    pub(crate) source_map: SourceMap,
    pub(crate) files: Vec<AnalysisFile>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) semantic: Arc<SemanticCache>,
}

impl AnalysisSnapshot {
    pub fn new(parts: ResolvedSnapshot, semantic: Arc<SemanticCache>) -> Self {
        let ResolvedSnapshot {
            source_map,
            mut files,
            diagnostics,
        } = parts;
        files.sort_by(|lhs, rhs| lhs.uri.cmp(&rhs.uri));
        Self {
            source_map,
            files,
            diagnostics,
            semantic,
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

    pub fn ast_files(&self) -> Vec<AstFile> {
        self.files.iter().map(|file| file.ast().clone()).collect()
    }

    pub fn semantic_diagnostics(&self) -> Vec<Diagnostic> {
        self.semantic.diagnostics()
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

    pub fn tir_analysis(&self) -> Option<&TirAnalysis> {
        self.semantic.tir()
    }

    pub fn shares_semantic_cache_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.semantic, &other.semantic)
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

    pub fn semantic_diagnostics(&self) -> Vec<Diagnostic> {
        self.snapshot().semantic_diagnostics()
    }

    pub fn ast_files(&self) -> Vec<AstFile> {
        self.snapshot().ast_files()
    }
}
