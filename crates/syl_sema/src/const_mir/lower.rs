use super::{ConstExpr, ConstLocalRef};
use crate::{
    const_eval::ConstKind,
    hir::{HirBodyExpr, HirExprNode},
    hir_resolve::HirResolution,
    hir_view::HirDesignViewExt,
    mir::{MirBinaryOp, MirTypeRef, MirUnaryOp},
    tir::TirDesign,
};
use std::collections::BTreeSet;
use syl_hir::{DefId, LocalId};
use syl_span::Span;

pub(super) struct ExprLowerer<'a> {
    tir: &'a TirDesign,
    owner: DefId,
    unsupported: bool,
    unsupported_span: Option<Span>,
    const_stack: BTreeSet<DefId>,
}

impl<'a> ExprLowerer<'a> {
    pub(super) fn new(tir: &'a TirDesign, owner: DefId) -> Self {
        Self {
            tir,
            owner,
            unsupported: false,
            unsupported_span: None,
            const_stack: tir
                .hir()
                .consts
                .contains_key(&owner)
                .then_some(owner)
                .into_iter()
                .collect(),
        }
    }

    pub(super) fn is_unsupported(&self) -> bool {
        self.unsupported
    }

    pub(super) fn unsupported_span(&self) -> Option<Span> {
        self.unsupported_span
    }

    pub(super) fn mark_unsupported(&mut self, span: Span) {
        self.unsupported = true;
        if self.unsupported_span.is_none() {
            self.unsupported_span = Some(span);
        }
    }

    pub(super) fn lower_local_assignment(
        &mut self,
        target: &HirBodyExpr,
        value: &HirBodyExpr,
    ) -> Option<(ConstLocalRef, ConstExpr)> {
        let HirExprNode::Ident(name) = &target.node else {
            return None;
        };
        Some((
            self.local_ref_for_expr(target, name),
            self.lower_expr(value),
        ))
    }

    pub(super) fn lower_expr(&mut self, expr: &HirBodyExpr) -> ConstExpr {
        match &expr.node {
            HirExprNode::Ident(name) => match self.tir.hir().expr_resolution(self.owner, expr) {
                Ok(Some(HirResolution::Def(def))) => {
                    let Some(item) = self.tir.hir().const_by_def(def) else {
                        return self.unsupported_expr(expr.span(), expr.id());
                    };
                    if !self.const_stack.insert(def) {
                        return self.unsupported_expr(expr.span(), expr.id());
                    }
                    let lowered = self.lower_expr(&item.value).with_origin(expr.id());
                    self.const_stack.remove(&def);
                    lowered
                }
                _ => ConstExpr::local(self.local_ref_for_expr(expr, name), expr.span())
                    .with_origin(expr.id()),
            },
            HirExprNode::Int(value) => ConstExpr::nat(*value, expr.span()).with_origin(expr.id()),
            HirExprNode::Bool(value) => {
                ConstExpr::bool_value(*value, expr.span()).with_origin(expr.id())
            }
            HirExprNode::Group(inner) => self.lower_expr(inner),
            HirExprNode::Unary {
                op, expr: inner, ..
            } => {
                let op = MirUnaryOp::from(*op);
                if matches!(op, MirUnaryOp::Unsupported) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::unary(op, self.lower_expr(inner), expr.span()).with_origin(expr.id())
            }
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                let op = MirBinaryOp::from(*op);
                if matches!(
                    op,
                    MirBinaryOp::Assign | MirBinaryOp::Field | MirBinaryOp::Unsupported
                ) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::binary(
                    op,
                    self.lower_expr(left),
                    self.lower_expr(right),
                    expr.span(),
                )
                .with_origin(expr.id())
            }
            HirExprNode::Call { callee, args } => {
                if let Some(call) = self.tir.extension_method_call(self.owner, callee)
                    && self.tir.hir().fns.contains_key(&call.method)
                {
                    let mut lowered_args = vec![self.lower_expr(call.receiver)];
                    lowered_args.extend(args.iter().map(|arg| self.lower_expr(&arg.value)));
                    return ConstExpr::call(call.method, lowered_args, expr.span())
                        .with_origin(expr.id());
                }
                let Some(root) = self.callee_root(callee) else {
                    return self.unsupported_expr(expr.span(), expr.id());
                };
                let Ok(Some(HirResolution::Def(def))) =
                    self.tir.hir().expr_resolution(self.owner, root)
                else {
                    return self.unsupported_expr(expr.span(), expr.id());
                };
                if !self.tir.hir().fns.contains_key(&def) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::call(
                    def,
                    args.iter().map(|arg| self.lower_expr(&arg.value)).collect(),
                    expr.span(),
                )
                .with_origin(expr.id())
            }
            HirExprNode::Field { .. } => self
                .enum_variant_expr(expr)
                .unwrap_or_else(|| self.unsupported_expr(expr.span(), expr.id())),
            HirExprNode::GenericApp { callee, .. } => self.lower_expr(callee),
            HirExprNode::Unsupported => self.unsupported_expr(expr.span(), expr.id()),
            _ => self.unsupported_expr(expr.span(), expr.id()),
        }
    }

    pub(super) fn local_ref_for_decl(&self, id: Option<LocalId>, name: &str) -> ConstLocalRef {
        ConstLocalRef::new(id, name.to_string())
    }

    pub(super) fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
        let mut current = ty;
        loop {
            if let Some(name) = current.path_name() {
                return match name {
                    "Nat" => Some(ConstKind::Nat),
                    "Bool" => Some(ConstKind::Bool),
                    _ => None,
                };
            }
            if let Some(base) = current.generic_base() {
                current = base;
                continue;
            }
            if let Some((base, _)) = current.view_select() {
                current = base;
                continue;
            }
            if let Some((_, elem)) = current.array() {
                current = elem;
                continue;
            }
            return None;
        }
    }

    fn unsupported_expr(&mut self, span: Span, origin: syl_hir::ExprId) -> ConstExpr {
        self.mark_unsupported(span);
        ConstExpr::unsupported(span).with_origin(origin)
    }

    fn enum_variant_expr(&mut self, expr: &HirBodyExpr) -> Option<ConstExpr> {
        let (enum_def, variant) = self.tir.hir().enum_variant_expr(expr)?;
        let value = self
            .tir
            .enum_variant_values()
            .get(&crate::hir::HirEnumVariantKey::new(enum_def, variant))?;
        Some(ConstExpr::nat(*value, expr.span()).with_origin(expr.id()))
    }

    fn callee_root<'b>(&self, expr: &'b HirBodyExpr) -> Option<&'b HirBodyExpr> {
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::Ident(_) => return Some(current),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }

    fn local_ref_for_expr(&self, expr: &HirBodyExpr, name: &str) -> ConstLocalRef {
        let id = self
            .tir
            .hir()
            .expr_resolution(self.owner, expr)
            .ok()
            .flatten()
            .and_then(|resolution| match resolution {
                HirResolution::Local(id) => Some(id),
                HirResolution::Def(_) => None,
                _ => None,
            });
        ConstLocalRef::new(id, name.to_string())
    }
}
