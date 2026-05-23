use std::{fmt, sync::OnceLock};
use syl_elab::{HirStage, HirStageOutput, MiddleCompiler, TirStage, TirStageOutput};
use syl_sema::StageOutput;
use syl_span::Diagnostic;
use syl_syntax::AstFile;

#[non_exhaustive]
pub struct SemanticCache {
    ast_files: Vec<AstFile>,
    hir: OnceLock<HirStageOutput>,
    tir: OnceLock<StageOutput<TirStage>>,
    downstream: OnceLock<TirStageOutput>,
    diagnostics: OnceLock<Vec<Diagnostic>>,
}

impl SemanticCache {
    pub fn new(ast_files: Vec<AstFile>) -> Self {
        Self {
            ast_files,
            hir: OnceLock::new(),
            tir: OnceLock::new(),
            downstream: OnceLock::new(),
            diagnostics: OnceLock::new(),
        }
    }

    pub(crate) fn hir_output(&self) -> &HirStageOutput {
        self.hir.get_or_init(|| {
            MiddleCompiler::new()
                .session(&self.ast_files)
                .resolve_hir_partial()
        })
    }

    pub(crate) fn hir(&self) -> &HirStage {
        self.hir_output().stage()
    }

    fn tir_output(&self) -> &StageOutput<TirStage> {
        self.tir.get_or_init(|| self.hir().check_tir_partial())
    }

    pub(crate) fn tir(&self) -> Option<&TirStage> {
        self.tir_output().partial_stage()
    }

    pub(crate) fn downstream_output(&self) -> Option<&TirStageOutput> {
        if !self.hir_output().diagnostics().is_empty() {
            return None;
        }
        if !self.tir_output().diagnostics().is_empty() {
            return None;
        }
        let tir = self.tir()?;
        Some(self.downstream.get_or_init(|| tir.downstream_output()))
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
                self.downstream_output()
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
            .field("downstream_cached", &self.downstream.get().is_some())
            .field("diagnostics_cached", &self.diagnostics.get().is_some())
            .finish()
    }
}
