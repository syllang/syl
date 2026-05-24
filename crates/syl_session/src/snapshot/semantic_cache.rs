use std::{fmt, sync::OnceLock};
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{HirAnalysis, HirAnalysisOutput, SemanticCompiler, StageOutput, TirAnalysis};
use syl_span::Diagnostic;
use syl_syntax::AstFile;

#[non_exhaustive]
pub struct SemanticCache {
    ast_files: Vec<AstFile>,
    hir: OnceLock<HirAnalysisOutput>,
    tir: OnceLock<StageOutput<TirAnalysis>>,
    elaboration: OnceLock<ElaborationOutput>,
    diagnostics: OnceLock<Vec<Diagnostic>>,
}

impl SemanticCache {
    pub fn new(ast_files: Vec<AstFile>) -> Self {
        Self {
            ast_files,
            hir: OnceLock::new(),
            tir: OnceLock::new(),
            elaboration: OnceLock::new(),
            diagnostics: OnceLock::new(),
        }
    }

    pub(crate) fn hir_output(&self) -> &HirAnalysisOutput {
        self.hir.get_or_init(|| {
            SemanticCompiler::new()
                .session(&self.ast_files)
                .resolve_hir_partial()
        })
    }

    pub(crate) fn hir(&self) -> &HirAnalysis {
        self.hir_output().stage()
    }

    fn tir_output(&self) -> &StageOutput<TirAnalysis> {
        self.tir.get_or_init(|| self.hir().check_tir_partial())
    }

    pub(crate) fn tir(&self) -> Option<&TirAnalysis> {
        self.tir_output().partial_stage()
    }

    pub(crate) fn elaboration_output(&self) -> Option<&ElaborationOutput> {
        if !self.hir_output().diagnostics().is_empty() {
            return None;
        }
        if !self.tir_output().diagnostics().is_empty() {
            return None;
        }
        let tir = self.tir()?;
        Some(
            self.elaboration
                .get_or_init(|| HardwareCompiler::new().output_for_tir(tir)),
        )
    }

    pub(crate) fn diagnostics(&self) -> Vec<Diagnostic> {
        self.diagnostics
            .get_or_init(|| {
                if !self.hir_output().diagnostics().is_empty() {
                    return self.hir_output().diagnostics().to_vec();
                }
                let tir = self.tir_output();
                if !tir.diagnostics().is_empty() {
                    return tir.diagnostics().to_vec();
                }
                self.elaboration_output()
                    .map(|output| output.diagnostics().to_vec())
                    .unwrap_or_default()
            })
            .clone()
    }
}

impl fmt::Debug for SemanticCache {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SemanticCache")
            .field("ast_file_count", &self.ast_files.len())
            .field("hir_cached", &self.hir.get().is_some())
            .field("tir_cached", &self.tir.get().is_some())
            .field("elaboration_cached", &self.elaboration.get().is_some())
            .field("diagnostics_cached", &self.diagnostics.get().is_some())
            .finish()
    }
}
