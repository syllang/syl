pub(crate) use syl_sema::ir::const_mir::{ConstEvalEnv, ConstKind, ConstValue};

mod lower;

use crate::{
    CompileError,
    const_mir::{ConstExpr, ConstFunction},
    mir::MirTypeRef,
    program::{ElabExpr, ElabProgram},
};
use syl_hir::DefId;

pub(crate) trait ConstValueElaborator {
    fn elab_value(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError>;

    fn elab_bool(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<Option<bool>, CompileError>;

    fn require_elab_nat(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
        context: &str,
    ) -> Result<ConstValue, CompileError>;

    fn kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind>;

    fn lower_expr(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &ConstEvalEnv,
    ) -> Result<ConstExpr, CompileError>;

    fn function(&self, def: DefId) -> Option<&ConstFunction>;
}

#[allow(dead_code)]
pub(crate) trait ConstMirElabExt: ConstValueElaborator {}

impl<T> ConstMirElabExt for T where T: ConstValueElaborator + ?Sized {}
