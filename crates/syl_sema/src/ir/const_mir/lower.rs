use super::{ConstExpr, ConstLocalRef, ConstNamedExpr, ConstStructKind};
use crate::{
    hir::resolve::HirResolution,
    hir::{HirBlock, HirBodyExpr, HirExprNode, HirNamedExpr, HirStmt},
    ir::{
        const_mir::ConstKind,
        mir::{MirBinaryOp, MirTypeRef, MirUnaryOp},
    },
};
use std::collections::BTreeSet;
use syl_hir::{DefId, LocalId};
use syl_span::Span;

use super::ConstMirLoweringContext;

pub(super) struct ExprLowerer<'a> {
    ctx: &'a dyn ConstMirLoweringContext,
    owner: DefId,
    unsupported: bool,
    unsupported_span: Option<Span>,
    const_stack: BTreeSet<DefId>,
}

struct StructAssignmentRewrite {
    base: ConstExpr,
    kind: ConstStructKind,
    fields: Vec<String>,
    updated_value: ConstExpr,
    span: Span,
}

impl<'a> ExprLowerer<'a> {
    pub(super) fn new(ctx: &'a dyn ConstMirLoweringContext, owner: DefId) -> Self {
        Self {
            ctx,
            owner,
            unsupported: false,
            unsupported_span: None,
            const_stack: ctx
                .is_const_owner(owner)
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
        match &target.node {
            HirExprNode::Ident(name) => Some((
                self.local_ref_for_expr(target, name),
                self.lower_expr(value),
            )),
            HirExprNode::Field { .. } => self.lower_field_assignment(target, value),
            HirExprNode::Group(inner) => self.lower_local_assignment(inner, value),
            _ => None,
        }
    }

    pub(super) fn lower_expr(&mut self, expr: &HirBodyExpr) -> ConstExpr {
        match &expr.node {
            HirExprNode::Ident(name) => match self.ctx.expr_resolution(self.owner, expr) {
                Ok(Some(HirResolution::Def(def))) => {
                    let Some(item) = self.ctx.const_by_def(def) else {
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
                if matches!(op, MirBinaryOp::Assign | MirBinaryOp::Unsupported) {
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
                if let Some((method, receiver)) = self.ctx.extension_method_call(self.owner, callee)
                    && self.ctx.function_exists(method)
                {
                    let mut lowered_args = vec![self.lower_expr(receiver)];
                    lowered_args.extend(args.iter().map(|arg| self.lower_expr(&arg.value)));
                    return ConstExpr::call(method, lowered_args, expr.span())
                        .with_origin(expr.id());
                }
                let Some(root) = self.callee_root(callee) else {
                    return self.unsupported_expr(expr.span(), expr.id());
                };
                let Ok(Some(HirResolution::Def(def))) = self.ctx.expr_resolution(self.owner, root)
                else {
                    return self.unsupported_expr(expr.span(), expr.id());
                };
                if !self.ctx.function_exists(def) {
                    return self.unsupported_expr(expr.span(), expr.id());
                }
                ConstExpr::call(
                    def,
                    args.iter().map(|arg| self.lower_expr(&arg.value)).collect(),
                    expr.span(),
                )
                .with_origin(expr.id())
            }
            HirExprNode::Aggregate { ty, fields } => match self.const_kind_for_type(ty) {
                Some(ConstKind::Struct(kind)) => {
                    ConstExpr::aggregate(kind, self.lower_named_exprs(fields), expr.span())
                        .with_origin(expr.id())
                }
                _ => self.unsupported_expr(expr.span(), expr.id()),
            },
            HirExprNode::Field { base, field } => {
                self.enum_variant_expr(expr).unwrap_or_else(|| {
                    ConstExpr::field(self.lower_expr(base), field.clone(), expr.span())
                        .with_origin(expr.id())
                })
            }
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
                    "nat" => Some(ConstKind::Nat),
                    "bool" => Some(ConstKind::Bool),
                    _ => self.struct_kind_for_type(current).map(ConstKind::Struct),
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
        self.ctx
            .enum_variant_value(expr)
            .map(|value| ConstExpr::nat(value, expr.span()).with_origin(expr.id()))
    }

    fn lower_named_exprs(&mut self, fields: &[HirNamedExpr]) -> Vec<ConstNamedExpr> {
        fields
            .iter()
            .map(|field| ConstNamedExpr::new(field.name.clone(), self.lower_expr(&field.value)))
            .collect()
    }

    fn struct_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstStructKind> {
        self.ctx
            .hir()
            .type_def_for_mir_type(self.owner, ty)
            .filter(|def| self.ctx.hir().structs.contains_key(def))
            .map(ConstStructKind::new)
    }

    fn lower_field_assignment(
        &mut self,
        target: &HirBodyExpr,
        value: &HirBodyExpr,
    ) -> Option<(ConstLocalRef, ConstExpr)> {
        let (root_expr, local, fields) = self.local_field_path(target)?;
        let root_kind = self.struct_kind_for_expr(root_expr)?;
        let root_name = match &root_expr.node {
            HirExprNode::Ident(name) => name,
            _ => return None,
        };
        let base = ConstExpr::local(local.clone(), root_expr.span()).with_origin(root_expr.id());
        let updated_value = self.lower_expr(value);
        let rebuilt = self.rebuild_struct_assignment(StructAssignmentRewrite {
            base,
            kind: root_kind,
            fields,
            updated_value,
            span: target.span(),
        })?;
        Some((self.local_ref_for_expr(root_expr, root_name), rebuilt))
    }

    fn local_field_path<'b>(
        &self,
        expr: &'b HirBodyExpr,
    ) -> Option<(&'b HirBodyExpr, ConstLocalRef, Vec<String>)> {
        let mut current = expr;
        let mut fields = Vec::new();
        loop {
            match &current.node {
                HirExprNode::Field { base, field } => {
                    fields.push(field.clone());
                    current = base;
                }
                HirExprNode::Group(inner) => current = inner,
                HirExprNode::Ident(name) => {
                    fields.reverse();
                    return self
                        .resolved_local_ref(current, name)
                        .map(|local| (current, local, fields));
                }
                _ => return None,
            }
        }
    }

    fn rebuild_struct_assignment(&mut self, rewrite: StructAssignmentRewrite) -> Option<ConstExpr> {
        let target_field = rewrite.fields.first()?;
        let struct_item = self.ctx.hir().structs.get(&rewrite.kind.def())?;
        let mut updated_value = Some(rewrite.updated_value);
        let fields = struct_item
            .fields
            .iter()
            .map(|field| {
                let field_name = field.name.clone();
                let value = if &field_name == target_field {
                    if rewrite.fields.len() == 1 {
                        updated_value.take()?
                    } else {
                        let ConstKind::Struct(child_kind) = self.const_kind_for_type(&field.ty)?
                        else {
                            return None;
                        };
                        self.rebuild_struct_assignment(StructAssignmentRewrite {
                            base: ConstExpr::field(
                                rewrite.base.clone(),
                                field_name.clone(),
                                rewrite.span,
                            ),
                            kind: child_kind,
                            fields: rewrite.fields[1..].to_vec(),
                            updated_value: updated_value.take()?,
                            span: rewrite.span,
                        })?
                    }
                } else {
                    ConstExpr::field(rewrite.base.clone(), field_name.clone(), rewrite.span)
                };
                Some(ConstNamedExpr::new(field_name, value))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ConstExpr::aggregate(rewrite.kind, fields, rewrite.span))
    }

    fn struct_kind_for_expr(&self, expr: &HirBodyExpr) -> Option<ConstStructKind> {
        self.ctx
            .expr_type(self.owner, expr)
            .and_then(|ty| ty.definition())
            .or_else(|| match &expr.node {
                HirExprNode::Ident(name) => self
                    .resolved_local_ref(expr, name)
                    .and_then(|local| local.id())
                    .and_then(|local| self.struct_kind_for_local(local, &mut BTreeSet::new())),
                _ => None,
            })
            .filter(|def| self.ctx.hir().structs.contains_key(def))
            .map(ConstStructKind::new)
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
            .ctx
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

    fn resolved_local_ref(&self, expr: &HirBodyExpr, name: &str) -> Option<ConstLocalRef> {
        self.ctx
            .expr_resolution(self.owner, expr)
            .ok()
            .flatten()
            .and_then(|resolution| match resolution {
                HirResolution::Local(id) => Some(id),
                _ => None,
            })
            .or_else(|| {
                self.ctx
                    .hir()
                    .locals
                    .iter()
                    .filter(|local| {
                        local.owner == self.owner
                            && local.name == name
                            && local.span.start <= expr.span().start
                    })
                    .max_by_key(|local| local.span.start)
                    .map(|local| local.id)
            })
            .map(|id| ConstLocalRef::new(Some(id), name.to_string()))
    }

    fn struct_kind_for_local(
        &self,
        local: LocalId,
        visited: &mut BTreeSet<LocalId>,
    ) -> Option<DefId> {
        if !visited.insert(local) {
            return None;
        }
        let function = self.ctx.hir().fns.get(&self.owner)?;
        let from_params = function
            .params
            .iter()
            .find(|param| param.id == Some(local))
            .and_then(|param| self.struct_kind_for_type(&param.ty))
            .map(ConstStructKind::def);
        let from_body = self.struct_kind_for_block_local(&function.body, local, visited);
        visited.remove(&local);
        from_params.or(from_body)
    }

    fn struct_kind_for_block_local(
        &self,
        block: &HirBlock,
        local: LocalId,
        visited: &mut BTreeSet<LocalId>,
    ) -> Option<DefId> {
        for stmt in &block.stmts {
            match stmt {
                HirStmt::Const { id, ty, value, .. }
                | HirStmt::Let {
                    id,
                    ty,
                    value: Some(value),
                    ..
                }
                | HirStmt::Var {
                    id,
                    ty,
                    value: Some(value),
                    ..
                } if *id == Some(local) => {
                    return self.struct_kind_for_decl(ty.as_ref(), Some(value), visited);
                }
                HirStmt::ElabIf {
                    then_block,
                    else_block,
                    ..
                } => {
                    if let Some(def) = self.struct_kind_for_block_local(then_block, local, visited)
                    {
                        return Some(def);
                    }
                    if let Some(def) = else_block
                        .as_ref()
                        .and_then(|block| self.struct_kind_for_block_local(block, local, visited))
                    {
                        return Some(def);
                    }
                }
                HirStmt::While { body, .. } | HirStmt::ElabFor { body, .. } => {
                    if let Some(def) = self.struct_kind_for_block_local(body, local, visited) {
                        return Some(def);
                    }
                }
                _ => {}
            }
        }
        None
    }

    fn struct_kind_for_decl(
        &self,
        ty: Option<&MirTypeRef>,
        value: Option<&HirBodyExpr>,
        visited: &mut BTreeSet<LocalId>,
    ) -> Option<DefId> {
        ty.and_then(|ty| self.struct_kind_for_type(ty))
            .map(ConstStructKind::def)
            .or_else(|| value.and_then(|expr| self.struct_kind_for_initializer(expr, visited)))
    }

    fn struct_kind_for_initializer(
        &self,
        expr: &HirBodyExpr,
        visited: &mut BTreeSet<LocalId>,
    ) -> Option<DefId> {
        match &expr.node {
            HirExprNode::Aggregate { ty, .. } => {
                self.struct_kind_for_type(ty).map(ConstStructKind::def)
            }
            HirExprNode::Ident(name) => self
                .resolved_local_ref(expr, name)
                .and_then(|local| local.id())
                .and_then(|local| self.struct_kind_for_local(local, visited)),
            HirExprNode::Group(inner) => self.struct_kind_for_initializer(inner, visited),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests;
