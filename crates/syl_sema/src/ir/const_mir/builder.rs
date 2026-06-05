use super::{
    BasicBlock, BlockId, ConstExpr, ConstFunction, ConstFunctionParts, ConstLocal, ConstMirBuilder,
    ConstMirLoweringContext, ConstMirProgram, ConstStmt, ConstStructDef, ConstStructFieldDef,
    ConstStructKind, Terminator, lower::ExprLowerer,
};
use crate::{
    CompileError,
    hir::{HirBlock, HirBodyExpr, HirFnItem, HirStmt},
    tir::TirDesign,
};
use std::collections::BTreeMap;
use syl_hir::DefId;

impl<'a> ConstMirBuilder<'a> {
    pub fn new(tir: &'a TirDesign) -> Self {
        Self { ctx: tir }
    }

    #[cfg(test)]
    pub(crate) fn with_context(ctx: &'a dyn ConstMirLoweringContext) -> Self {
        Self { ctx }
    }

    pub fn build(&self) -> Result<ConstMirProgram, CompileError> {
        let mut function_index = BTreeMap::new();
        let mut functions = Vec::new();
        for (owner, item) in &self.ctx.hir().fns {
            function_index.insert(*owner, functions.len());
            functions.push(self.lower_fn(*owner, item));
        }
        let mut structs = BTreeMap::new();
        let mut struct_path_index = BTreeMap::new();
        for (def, item) in &self.ctx.hir().structs {
            let kind = ConstStructKind::new(*def);
            let fields = item
                .fields
                .iter()
                .map(|field| {
                    let kind = self
                        .ctx
                        .hir()
                        .type_def_for_mir_type(*def, &field.ty)
                        .filter(|field_def| self.ctx.hir().structs.contains_key(field_def))
                        .map(ConstStructKind::new)
                        .map(super::ConstKind::Struct)
                        .or_else(|| match field.ty.type_name() {
                            Some("nat") => Some(super::ConstKind::Nat),
                            Some("bool") => Some(super::ConstKind::Bool),
                            _ => None,
                        });
                    ConstStructFieldDef::new(field.name.clone(), kind)
                })
                .collect();
            structs.insert(*def, ConstStructDef::new(kind, item.name.clone(), fields));
            if let Some(canonical) = self.ctx.hir().defs.get(def.get()) {
                struct_path_index.insert(canonical.canonical_path.segments().to_vec(), *def);
            }
        }
        Ok(ConstMirProgram {
            functions,
            function_index,
            structs,
            struct_path_index,
        })
    }

    fn lower_fn(&self, owner: DefId, item: &HirFnItem) -> ConstFunction {
        FunctionLowerer::new(self.ctx, owner, item).lower()
    }

    pub fn lower_const_expr(&self, owner: DefId, expr: &HirBodyExpr) -> ConstExpr {
        ExprLowerer::new(self.ctx, owner).lower_expr(expr)
    }
}

#[non_exhaustive]
struct FunctionLowerer<'a, 'b> {
    def: DefId,
    item: &'a HirFnItem,
    locals: Vec<ConstLocal>,
    blocks: Vec<BasicBlock>,
    exprs: ExprLowerer<'b>,
}

impl<'a, 'b> FunctionLowerer<'a, 'b> {
    fn new(ctx: &'b dyn ConstMirLoweringContext, def: DefId, item: &'a HirFnItem) -> Self {
        let locals = item
            .params
            .iter()
            .map(|param| ConstLocal::new(param.id, param.name.clone()))
            .collect();
        Self {
            def,
            item,
            locals,
            blocks: Vec::new(),
            exprs: ExprLowerer::new(ctx, def),
        }
    }

    fn lower(mut self) -> ConstFunction {
        let params = self
            .item
            .params
            .iter()
            .map(|param| param.name.clone())
            .collect::<Vec<_>>();
        let tail = self
            .item
            .body
            .tail
            .as_ref()
            .map(|expr| self.exprs.lower_expr(expr));
        let exit = self.push_block(Vec::new(), Terminator::Return(tail));
        let entry = self.lower_stmt_suffix(&self.item.body.stmts, 0, exit);
        ConstFunction::new(ConstFunctionParts {
            def: self.def,
            name: self.item.name.clone(),
            params,
            ret_kind: self
                .item
                .ret_ty
                .as_ref()
                .and_then(|ty| self.exprs.const_kind_for_type(&ty.ty)),
            locals: self.locals,
            blocks: self.blocks,
            entry,
            span: self.item.span,
            unsupported: self.exprs.is_unsupported(),
            unsupported_span: self.exprs.unsupported_span(),
        })
    }

    fn lower_stmt_suffix(&mut self, stmts: &[HirStmt], index: usize, next: BlockId) -> BlockId {
        let Some(stmt) = stmts.get(index) else {
            return next;
        };
        match stmt {
            HirStmt::Const {
                id,
                name,
                value,
                span: _span,
                ..
            }
            | HirStmt::Let {
                id,
                name,
                value: Some(value),
                span: _span,
                ..
            }
            | HirStmt::Var {
                id,
                name,
                value: Some(value),
                span: _span,
                ..
            } => {
                let local = self.exprs.local_ref_for_decl(*id, name);
                self.locals.push(ConstLocal::new(local.id(), name.clone()));
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                let value = self.exprs.lower_expr(value);
                self.push_block(
                    vec![ConstStmt::Assign { local, value }],
                    Terminator::Goto(rest),
                )
            }
            HirStmt::Assign { target, value, .. } => {
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                if let Some((local, value)) = self.exprs.lower_local_assignment(target, value) {
                    self.push_block(
                        vec![ConstStmt::Assign { local, value }],
                        Terminator::Goto(rest),
                    )
                } else {
                    self.exprs.mark_unsupported(target.span());
                    rest
                }
            }
            HirStmt::Let {
                id,
                name,
                ty,
                value: None,
                span,
                ..
            }
            | HirStmt::Var {
                id,
                name,
                ty,
                value: None,
                span,
                ..
            } => {
                let local = self.exprs.local_ref_for_decl(*id, name);
                self.locals.push(ConstLocal::new(local.id(), name.clone()));
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                let value = ty
                    .as_ref()
                    .and_then(|ty| self.exprs.const_kind_for_type(ty))
                    .map(|kind| {
                        ConstExpr::unknown(kind, ty.as_ref().map(|ty| ty.span()).unwrap_or(*span))
                    })
                    .unwrap_or_else(|| {
                        self.exprs.mark_unsupported(*span);
                        ConstExpr::unsupported(*span)
                    });
                self.push_block(
                    vec![ConstStmt::Assign { local, value }],
                    Terminator::Goto(rest),
                )
            }
            HirStmt::Expr(expr) => {
                self.exprs.mark_unsupported(expr.span());
                self.lower_stmt_suffix(stmts, index + 1, next)
            }
            HirStmt::ElabIf {
                cond,
                then_block,
                else_block,
                ..
            } => {
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                let then_entry = self.lower_block_to(then_block, rest);
                let else_entry = else_block
                    .as_ref()
                    .map(|block| self.lower_block_to(block, rest))
                    .unwrap_or(rest);
                let cond = self.exprs.lower_expr(cond);
                self.push_block(
                    Vec::new(),
                    Terminator::Branch {
                        cond,
                        then_block: then_entry,
                        else_block: else_entry,
                    },
                )
            }
            HirStmt::While { cond, body, .. } => {
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                let header = self.push_block(Vec::new(), Terminator::Goto(rest));
                let body_entry = self.lower_block_to(body, header);
                let cond = self.exprs.lower_expr(cond);
                self.set_terminator(
                    header,
                    Terminator::Branch {
                        cond,
                        then_block: body_entry,
                        else_block: rest,
                    },
                );
                header
            }
            HirStmt::Return(value, _) => {
                let value = value.as_ref().map(|expr| self.exprs.lower_expr(expr));
                self.push_block(Vec::new(), Terminator::Return(value))
            }
            _ => {
                self.exprs.mark_unsupported(stmt.span());
                self.lower_stmt_suffix(stmts, index + 1, next)
            }
        }
    }

    fn lower_block_to(&mut self, block: &HirBlock, next: BlockId) -> BlockId {
        let next = if let Some(tail) = &block.tail {
            self.exprs.mark_unsupported(tail.span());
            next
        } else {
            next
        };
        self.lower_stmt_suffix(&block.stmts, 0, next)
    }

    fn push_block(&mut self, stmts: Vec<ConstStmt>, term: Terminator) -> BlockId {
        let id = BlockId::new(self.blocks.len());
        self.blocks.push(BasicBlock::new(id, stmts, term));
        id
    }

    fn set_terminator(&mut self, id: BlockId, term: Terminator) {
        if let Some(block) = self.blocks.get_mut(id.index) {
            block.term = term;
        }
    }
}
