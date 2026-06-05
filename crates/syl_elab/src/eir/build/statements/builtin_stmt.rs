use crate::{
    CompileError, EirError,
    eir::{EirBinaryOp, EirExpr, EirItem, EirUnaryOp},
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

    pub(super) fn emit_assert_stmt(
        &self,
        expr: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        if args.len() != 1 || args[0].name.is_some() {
            return Err(CompileError::lowering_at(
                EirError::AssertionRequiresSingleCondition,
                expr.span(),
            ));
        }
        let clock = env.single_by_type("Clock", self).ok_or_else(|| {
            CompileError::lowering_at(EirError::AssertionRequiresClock, expr.span())
        })?;
        let mut reads = self.elab_read_places(&args[0].value, env);
        let trigger = self.assertion_trigger(&args[0].value, env, &mut reads);
        reads.sort_by_key(EirExpr::fact_key);
        reads.dedup_by_key(|read| read.fact_key());
        Ok(vec![EirItem::ClockedAssert {
            clock,
            trigger,
            reads,
            message: EirExpr::Str("assert failed".to_string()),
            origin: env.origin(expr.span()),
        }])
    }

    fn assertion_trigger(
        &self,
        condition: &ElabExpr,
        env: &Env,
        reads: &mut Vec<EirExpr>,
    ) -> EirExpr {
        let failed = EirExpr::unary(EirUnaryOp::Not, self.elab_expr(condition, env));
        if let Some(reset) = env.reset_for_unique_clock(self) {
            reads.push(reset.clone());
            return EirExpr::binary(
                EirBinaryOp::BitAnd,
                EirExpr::unary(EirUnaryOp::Not, reset),
                failed,
            );
        }
        failed
    }

    pub(super) fn runtime_error_stmt_args<'b>(
        &self,
        expr: &'b ElabExpr,
        env: &Env,
    ) -> Option<&'b [ElabCallArg]> {
        self.builtin_stmt_args(expr, env, "error")
    }

    pub(super) fn assertion_stmt_args<'b>(
        &self,
        expr: &'b ElabExpr,
        env: &Env,
    ) -> Option<&'b [ElabCallArg]> {
        self.builtin_stmt_args(expr, env, "assert")
    }

    fn builtin_stmt_args<'b>(
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
