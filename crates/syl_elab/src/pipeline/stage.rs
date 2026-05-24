use super::{ConstMirStage, EirBuildStage, MapIrStage};
use crate::{CompileError, eir_build::Elaborator, program::ElabProgram, tir::TirDesign};

#[non_exhaustive]
pub struct ElabStage {
    program: ElabProgram,
}

impl ElabStage {
    pub(super) fn from_tir(tir: &TirDesign) -> Self {
        Self {
            program: ElabProgram::from_tir(tir),
        }
    }

    pub fn build_raw_eir(
        &self,
        const_mir: &ConstMirStage,
        map_ir: &MapIrStage,
    ) -> Result<EirBuildStage, CompileError> {
        Elaborator::new(&self.program, &const_mir.program, &map_ir.program)
            .build_raw_design()
            .map(EirBuildStage::new)
    }
}
