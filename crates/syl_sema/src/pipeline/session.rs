use super::{
    analysis::HirAnalysis,
    input::SemanticSourceFile,
    output::{HirAnalysisOutput, SemanticOutput},
};
use crate::{CompileError, HirResolver};
use syl_span::Diagnostic;
use syl_syntax::AstFile;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SemanticCompiler;

impl SemanticCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn session<'files>(&self, files: &'files [AstFile]) -> SemanticSession<'files> {
        SemanticSession::new(files)
    }

    pub fn session_sources<'files>(
        &self,
        sources: Vec<SemanticSourceFile<'files>>,
    ) -> SemanticSession<'files> {
        SemanticSession::new_sources(sources)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticSession<'files> {
    sources: Vec<SemanticSourceFile<'files>>,
}

impl<'files> SemanticSession<'files> {
    pub fn new(files: &'files [AstFile]) -> Self {
        let sources = files
            .iter()
            .enumerate()
            .map(|(index, ast)| SemanticSourceFile::new(vec![format!("file{index}")], ast))
            .collect();
        Self { sources }
    }

    pub fn new_sources(sources: Vec<SemanticSourceFile<'files>>) -> Self {
        Self { sources }
    }

    pub fn resolve_hir(&self) -> Result<HirAnalysis, CompileError> {
        HirResolver::new_sources(self.semantic_sources())
            .resolve()
            .map(HirAnalysis::new)
    }

    pub fn resolve_hir_partial(&self) -> HirAnalysisOutput {
        let (design, errors) = HirResolver::new_sources(self.semantic_sources()).resolve_partial();
        let diagnostics = errors.into_iter().map(Diagnostic::from).collect();
        HirAnalysisOutput::new(HirAnalysis::new(design), diagnostics)
    }

    pub fn check(&self) -> SemanticOutput {
        let hir = match self.resolve_hir_collect() {
            Ok(hir) => hir,
            Err(errors) => {
                return SemanticOutput::new(
                    None,
                    errors.into_iter().map(Diagnostic::from).collect(),
                );
            }
        };
        let tir = hir.check_tir_partial();
        let diagnostics = tir.diagnostics().to_vec();
        SemanticOutput::new(tir.into_stage(), diagnostics)
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.check().diagnostics().to_vec()
    }

    fn resolve_hir_collect(&self) -> Result<HirAnalysis, Vec<CompileError>> {
        HirResolver::new_sources(self.semantic_sources())
            .resolve_collect()
            .map(HirAnalysis::new)
    }

    fn semantic_sources(&self) -> Vec<SemanticSourceFile<'files>> {
        self.sources
            .iter()
            .map(|source| SemanticSourceFile::new(source.module_path().to_vec(), source.ast()))
            .collect()
    }
}
