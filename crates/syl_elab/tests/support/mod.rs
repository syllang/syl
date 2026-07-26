use syl_elab::{CompileError, ElaborationOutput, HardwareCompiler};
use syl_hw::ParametricHwDesign;
use syl_sema::{OpaqueSummaryTable, SemanticSession};
use syl_syntax::AstFile;

#[derive(Debug, Default)]
pub struct MiddleCompiler {
    hardware: HardwareCompiler,
}

impl MiddleCompiler {
    pub fn new() -> Self {
        Self {
            hardware: HardwareCompiler::new(),
        }
    }

    #[allow(dead_code)]
    pub fn with_opaque_summaries(opaque_summaries: OpaqueSummaryTable) -> Self {
        Self {
            hardware: HardwareCompiler::with_opaque_summaries(opaque_summaries),
        }
    }

    #[allow(dead_code)]
    pub fn compile_files(&self, files: &[AstFile]) -> Result<ParametricHwDesign, CompileError> {
        let hir = SemanticSession::new(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        self.hardware.compile_tir(&tir)
    }

    #[allow(dead_code)]
    pub fn output_files(&self, files: &[AstFile]) -> Result<ElaborationOutput, CompileError> {
        let hir = SemanticSession::new(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        Ok(self.hardware.output_for_tir(&tir))
    }

    #[allow(dead_code)]
    pub fn session<'files>(&self, files: &'files [AstFile]) -> SemanticSession<'files> {
        SemanticSession::new(files)
    }
}
