use super::context::CapabilityContext;
use crate::{
    hir::resolve::HirResolution,
    hir::{HirBodyExpr, HirCallable, HirDefKind, HirExprNode},
};
use syl_hir::DefId;

/// Resolves expression callees to the callable declarations used by capability rules.
///
/// Keeping this lookup in one place makes the boundary explicit: capability checking
/// consumes only cells and extern cells, while expression walking remains responsible
/// for deciding how the callable's signature is applied.
pub(super) struct CallableResolver<'a> {
    ctx: &'a dyn CapabilityContext,
}

pub(super) struct ResolvedCallable<'a> {
    pub(super) def: DefId,
    pub(super) name: String,
    pub(super) callable: &'a HirCallable,
}

impl<'a> CallableResolver<'a> {
    pub(super) fn new(ctx: &'a dyn CapabilityContext) -> Self {
        Self { ctx }
    }

    pub(super) fn resolve_value(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Option<ResolvedCallable<'a>> {
        match &expr.node {
            HirExprNode::Call { callee, .. } | HirExprNode::Place { callee, .. } => {
                self.resolve_callee(owner, callee)
            }
            _ => None,
        }
    }

    pub(super) fn resolve_callee(
        &self,
        owner: DefId,
        callee: &HirBodyExpr,
    ) -> Option<ResolvedCallable<'a>> {
        let root = callee_root(callee)?;
        let Some(HirResolution::Def(def)) = self.ctx.expr_resolution(owner, root).ok()? else {
            return None;
        };
        let kind = self.ctx.def_kind(def)?;
        if !matches!(kind, HirDefKind::Cell | HirDefKind::ExternCell) {
            return None;
        }
        let name = self.ctx.def_name(def)?.to_string();
        let callable = self.ctx.callable_by_def(def)?;
        Some(ResolvedCallable {
            def,
            name,
            callable,
        })
    }
}

fn callee_root(mut callee: &HirBodyExpr) -> Option<&HirBodyExpr> {
    loop {
        match &callee.node {
            HirExprNode::Ident(_) => return Some(callee),
            HirExprNode::GenericApp { callee: inner, .. } | HirExprNode::Group(inner) => {
                callee = inner;
            }
            _ => return None,
        }
    }
}
