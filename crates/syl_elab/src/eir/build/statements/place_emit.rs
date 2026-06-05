use crate::{
    CompileError,
    eir::EirItem,
    program::{ElabCallArg, ElabExpr, ElabExprNode},
};
use syl_span::Span;

use super::super::connections::InstanceEmitRequest;
use super::{EirBuilder, Env, ExprPlaceEmit};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn emit_expr_place(
        &self,
        request: ExprPlaceEmit<'_>,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let inst_name = self.expr_place_inst_name(&request, env);
        self.emit_named_expr_place(
            &inst_name,
            request.callee,
            request.args,
            request.inplace,
            request.span,
            env,
        )
    }

    pub(super) fn emit_named_expr_place(
        &self,
        inst_name: &str,
        callee: &ElabExpr,
        args: &[ElabCallArg],
        inplace: bool,
        span: Span,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut items = Vec::new();
        if let Some(result_ty) = self.callable_result_type_from_elab(callee, env) {
            let mut signal_env = env.clone();
            items.extend(self.emit_result_signals(inst_name, &result_ty, span, &mut signal_env));
        }
        items.extend(self.emit_instance(InstanceEmitRequest {
            inst_name,
            callee,
            args,
            env,
            inplace,
            span,
        })?);
        Ok(items)
    }

    fn expr_place_inst_name(&self, request: &ExprPlaceEmit<'_>, env: &Env) -> String {
        let leaf = self.explicit_place_name(request);
        if let Some(prefix) = env.expr_place_prefix.as_deref() {
            return format!("{prefix}_{leaf}");
        }
        if let Some(prefix) = env.prefix.as_deref() {
            return format!("{prefix}_{leaf}");
        }

        leaf
    }

    fn explicit_place_name(&self, request: &ExprPlaceEmit<'_>) -> String {
        self.callee_ident_name(request.callee)
            .map(|name| format!("{name}_{}", request.span.start))
            .unwrap_or_else(|| format!("place_{}", request.span.start))
    }

    fn callee_ident_name(&self, expr: &crate::program::ElabExpr) -> Option<String> {
        match &expr.node {
            ElabExprNode::Ident(name) => Some(self.sanitize(name)),
            ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                self.callee_ident_name(callee)
            }
            _ => None,
        }
    }
}
