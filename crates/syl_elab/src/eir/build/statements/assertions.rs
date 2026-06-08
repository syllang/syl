use crate::{
    CompileError, EirError,
    eir::{EirBinaryOp, EirExpr, EirItem, EirUnaryOp},
    program::{ElabCallArg, ElabExpr},
};

use super::{EirBuilder, Env};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn try_emit_assert_stmt(
        &self,
        expr: &ElabExpr,
        env: &Env,
        allow_assert_builtin: bool,
    ) -> Result<Option<Vec<EirItem>>, CompileError> {
        let Some(args) = self.assertion_stmt_args(expr, env) else {
            return Ok(None);
        };
        if !allow_assert_builtin {
            return Err(CompileError::lowering_at(
                EirError::AssertionStatementOnly,
                expr.span(),
            ));
        }
        self.emit_assert_stmt(expr, args, env).map(Some)
    }

    fn emit_assert_stmt(
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

    fn assertion_stmt_args<'b>(&self, expr: &'b ElabExpr, env: &Env) -> Option<&'b [ElabCallArg]> {
        self.builtin_stmt_args(expr, env, "assert")
    }
}
