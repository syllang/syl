use crate::{
    CompileError, EirError,
    eir::EirItem,
    program::{ElabCallArg, ElabExpr, ElabExprNode, ElabResolution},
};

use super::{EirBuilder, Env};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn emit_runtime_error_stmt(
        &self,
        expr: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        if args.len() != 1 || args[0].name.is_some() {
            return Err(CompileError::lowering_at(
                EirError::RuntimeErrorRequiresSingleMessage,
                expr.span(),
            ));
        }
        Ok(vec![EirItem::InitialError {
            message: self.elab_expr(&args[0].value, env),
            origin: env.origin(expr.span()),
        }])
    }

    pub(super) fn runtime_error_stmt_args<'b>(
        &self,
        expr: &'b ElabExpr,
        env: &Env,
    ) -> Option<&'b [ElabCallArg]> {
        self.builtin_stmt_args(expr, env, "error")
    }

    pub(super) fn builtin_stmt_args<'b>(
        &self,
        expr: &'b ElabExpr,
        env: &Env,
        builtin_name: &str,
    ) -> Option<&'b [ElabCallArg]> {
        let ElabExprNode::Call { callee, args } = &expr.node else {
            return None;
        };
        let root = self.elab_callee_root(callee)?;
        if let Some(owner) = env.owner
            && matches!(
                self.program.expr_resolution(owner, root),
                Some(ElabResolution::Def(_) | ElabResolution::Local(_))
            )
        {
            return None;
        }
        let ElabExprNode::Ident(name) = &root.node else {
            return None;
        };
        (name == builtin_name).then_some(args.as_slice())
    }
}
