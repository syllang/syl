use crate::{
    CompileError,
    eir::EirItem,
    program::{ElabCallArg, ElabExpr, ElabExprNode},
};
use syl_span::Span;

use super::super::connections::InstanceEmitRequest;
use super::{EirBuilder, Env, ExprPlaceEmit};

#[non_exhaustive]
pub(super) struct NamedExprPlaceEmit<'a> {
    pub(super) inst_name: &'a str,
    pub(super) callee: &'a ElabExpr,
    pub(super) args: &'a [ElabCallArg],
    pub(super) inplace: bool,
    pub(super) span: Span,
}

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
            NamedExprPlaceEmit {
                inst_name: &inst_name,
                callee: request.callee,
                args: request.args,
                inplace: request.inplace,
                span: request.span,
            },
            env,
        )
    }

    pub(super) fn emit_named_expr_place(
        &self,
        request: NamedExprPlaceEmit<'_>,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut items = Vec::new();
        if let Some(result_ty) = self.callable_result_type_from_elab(request.callee, env) {
            let mut signal_env = env.clone();
            items.extend(self.emit_result_signals(
                request.inst_name,
                &result_ty,
                request.span,
                &mut signal_env,
            ));
        }
        items.extend(self.emit_instance(InstanceEmitRequest {
            inst_name: request.inst_name,
            callee: request.callee,
            args: request.args,
            env,
            inplace: request.inplace,
            span: request.span,
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
