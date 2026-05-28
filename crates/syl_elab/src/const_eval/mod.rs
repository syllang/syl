pub(crate) use syl_sema::ir::const_mir::{ConstEvalEnv, ConstKind, ConstValue};

mod lower;

use crate::{CompileError, mir::MirTypeRef, program::ElabExpr};

pub(crate) trait ConstValueElaborator {
    fn elab_value(
        &self,
        program: &crate::program::ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError>;

    fn elab_bool(
        &self,
        program: &crate::program::ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<Option<bool>, CompileError>;

    fn require_elab_nat(
        &self,
        program: &crate::program::ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
        context: &str,
    ) -> Result<ConstValue, CompileError>;

    fn kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind>;
}

#[allow(dead_code)]
pub(crate) trait ConstMirElabExt: ConstValueElaborator {}

impl<T> ConstMirElabExt for T where T: ConstValueElaborator + ?Sized {}
