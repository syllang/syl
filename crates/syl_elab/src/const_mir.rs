mod lower;

pub(crate) use syl_sema::ir::const_mir::{
    ConstExpr, ConstFunction, ConstMirBuilder, ConstMirProgram,
};

use crate::{
    CompileError,
    const_eval::{ConstEvalEnv, ConstValue},
    program::ElabExpr,
};

pub(crate) trait ConstMirElabExt {
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
}
