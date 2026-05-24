use syl_elab::{CompileError, ElaborationOutput, HardwareCompiler};
use syl_hw::ParametricHwDesign;
use syl_sema::{SemanticCompiler, SemanticSession};
use syl_syntax::AstFile;

#[derive(Debug, Default)]
pub struct MiddleCompiler {
    semantic: SemanticCompiler,
    hardware: HardwareCompiler,
}

impl MiddleCompiler {
    pub fn new() -> Self {
        Self {
            semantic: SemanticCompiler::new(),
            hardware: HardwareCompiler::new(),
        }
    }

    #[allow(dead_code)]
    pub fn compile_files(&self, files: &[AstFile]) -> Result<ParametricHwDesign, CompileError> {
        let hir = self.semantic.session(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        self.hardware.compile_tir(&tir)
    }

    #[allow(dead_code)]
    pub fn output_files(&self, files: &[AstFile]) -> Result<ElaborationOutput, CompileError> {
        let hir = self.semantic.session(files).resolve_hir()?;
        let tir = hir.check_tir()?;
        Ok(self.hardware.output_for_tir(&tir))
    }

    #[allow(dead_code)]
    pub fn session<'files>(&self, files: &'files [AstFile]) -> SemanticSession<'files> {
        self.semantic.session(files)
    }
}
