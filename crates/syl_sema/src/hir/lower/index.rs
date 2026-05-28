use crate::{
    hir::lower::HirResolver,
    hir::{
        HirBlock, HirBodyExpr, HirBundleItem, HirCallableItem, HirConstItem, HirExpr, HirExprNode,
        HirFieldAccess, HirFnItem, HirInterfaceItem, HirMapItem, HirRegReset,
        HirSignatureGenericParam, HirSignatureParam, HirStmt, HirTypeRef,
    },
    ir::mir::MirTypeRef,
};
use syl_hir::{DefId, ExprId};

impl<'files> HirResolver<'files> {
    pub(super) fn index_const(&mut self, owner: DefId, item: &mut HirConstItem) {
        self.index_optional_type(owner, &item.ty);
        self.index_expr(owner, &mut item.value);
    }

    pub(super) fn index_fn(&mut self, owner: DefId, item: &mut HirFnItem) {
        self.index_params(owner, &item.params);
        if let Some(ty) = &item.ret_ty {
            self.index_mir_type(owner, &ty.ty);
        }
        self.index_block(owner, &mut item.body);
    }

    pub(super) fn index_bundle(&mut self, owner: DefId, item: &mut HirBundleItem) {
        self.index_generics(owner, &mut item.generics);
        for attr in &mut item.attrs {
            for arg in &mut attr.args {
                self.index_expr(owner, arg);
            }
        }
        for field in &item.fields {
            self.index_mir_type(owner, &field.ty);
        }
    }

    pub(super) fn index_interface(&mut self, owner: DefId, item: &mut HirInterfaceItem) {
        self.index_generics(owner, &mut item.generics);
        for field in &item.fields {
            self.index_mir_type(owner, &field.ty);
        }
    }

    pub(super) fn index_map(&mut self, owner: DefId, item: &mut HirMapItem) {
        self.index_generics(owner, &mut item.generics);
        self.index_params(owner, &item.params);
        if let Some(ty) = &item.ret_ty {
            self.index_mir_type(owner, &ty.ty);
        }
        self.index_expr(owner, &mut item.body);
    }

    pub(super) fn index_callable(&mut self, owner: DefId, item: &mut HirCallableItem) {
        self.index_generics(owner, &mut item.generics);
        self.index_params(owner, &item.params);
        for port in &item.ports {
            self.index_mir_type(owner, &port.ty);
        }
        if let Some(result) = &item.result {
            self.index_mir_type(owner, &result.ty);
        }
        self.index_block(owner, &mut item.body);
    }

    pub(super) fn index_extern_cell(
        &mut self,
        owner: DefId,
        item: &mut crate::hir::HirExternCellItem,
    ) {
        self.index_generics(owner, &mut item.generics);
        self.index_params(owner, &item.params);
        for port in &item.ports {
            self.index_mir_type(owner, &port.ty);
        }
        if let Some(result) = &item.result {
            self.index_mir_type(owner, &result.ty);
        }
    }

    fn index_generics(&mut self, owner: DefId, generics: &mut [HirSignatureGenericParam]) {
        for generic in generics {
            self.index_optional_type(owner, &generic.kind);
            if let Some(default) = &mut generic.default {
                self.index_expr(owner, default);
            }
        }
    }

    fn index_params(&mut self, owner: DefId, params: &[HirSignatureParam]) {
        for param in params {
            self.index_mir_type(owner, &param.ty);
        }
    }

    fn index_block(&mut self, owner: DefId, block: &mut HirBlock) {
        for stmt in &mut block.stmts {
            self.index_stmt(owner, stmt);
        }
        if let Some(tail) = &mut block.tail {
            self.index_expr(owner, tail);
        }
    }

    fn index_stmt(&mut self, owner: DefId, stmt: &mut HirStmt) {
        match stmt {
            HirStmt::Const { ty, value, .. }
            | HirStmt::Let {
                ty,
                value: Some(value),
                ..
            }
            | HirStmt::Var {
                ty,
                value: Some(value),
                ..
            }
            | HirStmt::Signal {
                ty,
                value: Some(value),
                ..
            } => {
                self.index_optional_type(owner, ty);
                self.index_expr(owner, value);
            }
            HirStmt::Assign { target, value, .. } | HirStmt::Drive { target, value, .. } => {
                self.index_expr(owner, target);
                self.index_expr(owner, value);
            }
            HirStmt::Let {
                ty, value: None, ..
            }
            | HirStmt::Var {
                ty, value: None, ..
            }
            | HirStmt::Signal {
                ty, value: None, ..
            } => self.index_optional_type(owner, ty),
            HirStmt::Next { value, .. } => {
                self.index_expr(owner, value);
            }
            HirStmt::Reg { ty, reset, .. } => {
                self.index_optional_type(owner, ty);
                if let Some(reset) = reset {
                    self.index_reset(owner, reset);
                }
            }
            HirStmt::While { cond, body, .. } => {
                self.index_expr(owner, cond);
                self.index_block(owner, body);
            }
            HirStmt::ElabIf {
                cond,
                then_block,
                else_block,
                ..
            } => {
                self.index_expr(owner, cond);
                self.index_block(owner, then_block);
                if let Some(block) = else_block {
                    self.index_block(owner, block);
                }
            }
            HirStmt::ElabFor { range, body, .. } => {
                self.index_expr(owner, range);
                self.index_block(owner, body);
            }
            HirStmt::Expr(expr) => self.index_expr(owner, expr),
            HirStmt::Return(Some(expr), _) => self.index_expr(owner, expr),
            HirStmt::Return(None, _) | HirStmt::Error { .. } => {}
            _ => {}
        }
    }

    fn index_reset(&mut self, owner: DefId, reset: &mut HirRegReset) {
        if let Some(domain) = &mut reset.domain {
            self.index_expr(owner, domain);
        }
        self.index_expr(owner, &mut reset.value);
    }

    fn index_expr(&mut self, owner: DefId, expr: &mut HirBodyExpr) {
        let id = ExprId::new(self.design.exprs.len());
        expr.id = id;
        self.design.exprs.push(HirExpr::new(id, owner, expr.span()));
        let span = expr.span();
        match &mut expr.node {
            HirExprNode::Unary { expr, .. } | HirExprNode::Group(expr) => {
                self.index_expr(owner, expr);
            }
            HirExprNode::Binary { left, right, .. } => {
                self.index_expr(owner, left);
                self.index_expr(owner, right);
            }
            HirExprNode::Call { callee, args } | HirExprNode::Place { callee, args, .. } => {
                self.index_expr(owner, callee);
                for arg in args {
                    self.index_expr(owner, &mut arg.value);
                }
            }
            HirExprNode::GenericApp { callee, args } => {
                self.index_expr(owner, callee);
                for arg in args {
                    self.index_mir_type(owner, arg);
                }
            }
            HirExprNode::Aggregate { ty, fields } => {
                self.index_mir_type(owner, ty);
                for field in fields {
                    self.index_expr(owner, &mut field.value);
                }
            }
            HirExprNode::Field { base, field } => {
                self.index_expr(owner, base);
                self.design.field_accesses.push(HirFieldAccess::new(
                    owner,
                    base.as_ref().clone(),
                    field.clone(),
                    span,
                ));
            }
            HirExprNode::Index { base, index } => {
                self.index_expr(owner, base);
                self.index_expr(owner, index);
            }
            HirExprNode::Block(block) => self.index_block(owner, block),
            HirExprNode::Match { expr, arms } => {
                self.index_expr(owner, expr);
                for arm in arms {
                    self.index_expr(owner, &mut arm.value);
                }
            }
            HirExprNode::Select { arms, .. } => {
                for arm in arms {
                    self.index_expr(owner, &mut arm.pattern);
                    self.index_expr(owner, &mut arm.value);
                }
            }
            HirExprNode::CompileError { message } => self.index_expr(owner, message),
            HirExprNode::Range { start, end } => {
                self.index_expr(owner, start);
                self.index_expr(owner, end);
            }
            HirExprNode::For { range, body, .. } => {
                self.index_expr(owner, range);
                self.index_block(owner, body);
            }
            HirExprNode::Ident(_)
            | HirExprNode::Int(_)
            | HirExprNode::Str(_)
            | HirExprNode::Bool(_)
            | HirExprNode::Unsupported => {}
            _ => {}
        }
    }

    fn index_optional_type(&mut self, owner: DefId, ty: &Option<MirTypeRef>) {
        if let Some(ty) = ty {
            self.index_mir_type(owner, ty);
        }
    }

    fn index_mir_type(&mut self, owner: DefId, ty: &MirTypeRef) {
        self.design
            .type_refs
            .push(HirTypeRef::new(owner, ty.clone()));
        if let Some((_, elem)) = ty.array() {
            self.index_mir_type(owner, elem);
        }
        if let Some(base) = ty.generic_base() {
            self.index_mir_type(owner, base);
        }
        if let Some(args) = ty.args() {
            for arg in args {
                self.index_mir_type(owner, arg);
            }
        }
        if let Some((base, _)) = ty.view_select() {
            self.index_mir_type(owner, base);
        }
    }
}
