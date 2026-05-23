use super::{ConstMirStage, EirStage, MapIrStage};
use crate::{CompileError, eir::Elaborator, program::ElabProgram, tir::TirDesign};

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

    pub fn elaborate(
        &self,
        const_mir: &ConstMirStage,
        map_ir: &MapIrStage,
    ) -> Result<EirStage, CompileError> {
        Elaborator::new(&self.program, &const_mir.program, &map_ir.program)
            .elaborate()
            .map(EirStage::new)
    }
}
