use crate::{
    CompileError,
    const_eval::{ConstEvaluator, ConstKind},
    hir::{HirBlock, HirBodyExpr, HirExprNode, HirFnItem, HirStmt},
    hir_resolve::HirResolution,
    hir_view::HirDesignViewExt,
    mir::{MirBinaryOp, MirTypeRef, MirUnaryOp},
    tir::TirDesign,
};
use std::collections::BTreeMap;
use syl_hir::{DefId, LocalId};
use syl_span::Span;

mod metrics;

#[non_exhaustive]
pub struct ConstMirProgram {
    functions: Vec<ConstFunction>,
    function_index: BTreeMap<DefId, usize>,
}

impl ConstMirProgram {
    pub fn evaluator(&self) -> ConstEvaluator<'_> {
        ConstEvaluator::new(self)
    }

    pub fn function(&self, id: DefId) -> Option<&ConstFunction> {
        self.function_index
            .get(&id)
            .and_then(|idx| self.functions.get(*idx))
    }
}

#[non_exhaustive]
pub struct ConstFunction {
    def: DefId,
    name: String,
    params: Vec<String>,
    ret_kind: Option<ConstKind>,
    locals: Vec<ConstLocal>,
    blocks: Vec<BasicBlock>,
    entry: BlockId,
    span: Span,
    unsupported: bool,
    unsupported_span: Option<Span>,
}

struct ConstFunctionParts {
    def: DefId,
    name: String,
    params: Vec<String>,
    ret_kind: Option<ConstKind>,
    locals: Vec<ConstLocal>,
    blocks: Vec<BasicBlock>,
    entry: BlockId,
    span: Span,
    unsupported: bool,
    unsupported_span: Option<Span>,
}

impl ConstFunction {
    fn new(parts: ConstFunctionParts) -> Self {
        Self {
            def: parts.def,
            name: parts.name,
            params: parts.params,
            ret_kind: parts.ret_kind,
            locals: parts.locals,
            blocks: parts.blocks,
            entry: parts.entry,
            span: parts.span,
            unsupported: parts.unsupported,
            unsupported_span: parts.unsupported_span,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn def(&self) -> DefId {
        self.def
    }

    pub fn params(&self) -> &[String] {
        &self.params
    }

    pub fn ret_kind(&self) -> Option<ConstKind> {
        self.ret_kind
    }

    pub fn entry(&self) -> BlockId {
        self.entry
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(id.index)
    }

    pub fn is_unsupported(&self) -> bool {
        self.unsupported
    }

    pub fn unsupported_span(&self) -> Option<Span> {
        self.unsupported_span
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct BlockId {
    index: usize,
}

impl BlockId {
    fn new(index: usize) -> Self {
        Self { index }
    }
}

#[non_exhaustive]
pub struct ConstLocal {
    id: Option<LocalId>,
    name: String,
}

impl ConstLocal {
    fn new(id: Option<LocalId>, name: String) -> Self {
        Self { id, name }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct ConstLocalRef {
    id: Option<LocalId>,
    name: String,
}

impl ConstLocalRef {
    fn new(id: Option<LocalId>, name: String) -> Self {
        Self { id, name }
    }

    pub fn id(&self) -> Option<LocalId> {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[non_exhaustive]
pub struct BasicBlock {
    id: BlockId,
    stmts: Vec<ConstStmt>,
    term: Terminator,
}

impl BasicBlock {
    fn new(id: BlockId, stmts: Vec<ConstStmt>, term: Terminator) -> Self {
        Self { id, stmts, term }
    }

    pub fn stmts(&self) -> &[ConstStmt] {
        &self.stmts
    }

    pub fn terminator(&self) -> &Terminator {
        &self.term
    }
}

#[non_exhaustive]
pub enum ConstStmt {
    Assign {
        local: ConstLocalRef,
        value: ConstExpr,
    },
}

#[non_exhaustive]
pub enum Terminator {
    Goto(BlockId),
    Branch {
        cond: ConstExpr,
        then_block: BlockId,
        else_block: BlockId,
    },
    Return(Option<ConstExpr>),
}

#[non_exhaustive]
pub struct ConstExpr {
    kind: ConstExprKind,
    span: Span,
}

impl ConstExpr {
    fn new(kind: ConstExprKind, span: Span) -> Self {
        Self { kind, span }
    }

    pub fn local(local: ConstLocalRef, span: Span) -> Self {
        Self::new(ConstExprKind::Local(local), span)
    }

    pub fn named_local(name: impl Into<String>, span: Span) -> Self {
        Self::local(ConstLocalRef::new(None, name.into()), span)
    }

    pub fn unknown(kind: ConstKind, span: Span) -> Self {
        Self::new(ConstExprKind::Unknown(kind), span)
    }

    pub fn int(value: u64, span: Span) -> Self {
        Self::new(ConstExprKind::Int(value), span)
    }

    pub fn bool_value(value: bool, span: Span) -> Self {
        Self::new(ConstExprKind::Bool(value), span)
    }

    pub fn unary(op: MirUnaryOp, expr: ConstExpr, span: Span) -> Self {
        Self::new(
            ConstExprKind::Unary {
                op,
                expr: Box::new(expr),
            },
            span,
        )
    }

    pub fn binary(op: MirBinaryOp, left: ConstExpr, right: ConstExpr, span: Span) -> Self {
        Self::new(
            ConstExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        )
    }

    pub fn call(callee: DefId, args: Vec<ConstExpr>, span: Span) -> Self {
        Self::new(ConstExprKind::Call { callee, args }, span)
    }

    pub fn unsupported(span: Span) -> Self {
        Self::new(ConstExprKind::Unsupported, span)
    }

    pub fn kind(&self) -> &ConstExprKind {
        &self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[non_exhaustive]
pub enum ConstExprKind {
    Local(ConstLocalRef),
    Unknown(ConstKind),
    Int(u64),
    Bool(bool),
    Unary {
        op: MirUnaryOp,
        expr: Box<ConstExpr>,
    },
    Binary {
        op: MirBinaryOp,
        left: Box<ConstExpr>,
        right: Box<ConstExpr>,
    },
    Call {
        callee: DefId,
        args: Vec<ConstExpr>,
    },
    Unsupported,
}

#[non_exhaustive]
pub struct ConstMirBuilder<'a> {
    tir: &'a TirDesign,
}

impl<'a> ConstMirBuilder<'a> {
    pub fn new(tir: &'a TirDesign) -> Self {
        Self { tir }
    }

    pub fn build(self) -> Result<ConstMirProgram, CompileError> {
        let _typeck_summary = (self.tir.expr_phases().len(), self.tir.binding_kinds().len());
        let mut function_index = BTreeMap::new();
        let mut functions = Vec::new();
        for (owner, item) in &self.tir.hir().fns {
            function_index.insert(*owner, functions.len());
            functions.push(self.lower_fn(*owner, item));
        }
        Ok(ConstMirProgram {
            functions,
            function_index,
        })
    }

    fn lower_fn(&self, owner: DefId, item: &HirFnItem) -> ConstFunction {
        FunctionLowerer::new(self, owner, item).lower()
    }
}

#[non_exhaustive]
struct FunctionLowerer<'a, 'b> {
    owner: &'a ConstMirBuilder<'b>,
    def: DefId,
    item: &'a HirFnItem,
    locals: Vec<ConstLocal>,
    blocks: Vec<BasicBlock>,
    unsupported: bool,
    unsupported_span: Option<Span>,
}

impl<'a, 'b> FunctionLowerer<'a, 'b> {
    fn new(owner: &'a ConstMirBuilder<'b>, def: DefId, item: &'a HirFnItem) -> Self {
        let locals = item
            .params
            .iter()
            .map(|param| ConstLocal::new(param.id, param.name.clone()))
            .collect();
        Self {
            owner,
            def,
            item,
            locals,
            blocks: Vec::new(),
            unsupported: false,
            unsupported_span: None,
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
            .map(|expr| self.lower_expr(expr));
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
                .and_then(|ty| self.const_kind_for_type(&ty.ty)),
            locals: self.locals,
            blocks: self.blocks,
            entry,
            span: self.item.span,
            unsupported: self.unsupported,
            unsupported_span: self.unsupported_span,
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
                let local = self.local_ref_for_decl(*id, name);
                self.locals.push(ConstLocal::new(local.id(), name.clone()));
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                let value = self.lower_expr(value);
                self.push_block(
                    vec![ConstStmt::Assign { local, value }],
                    Terminator::Goto(rest),
                )
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
                let local = self.local_ref_for_decl(*id, name);
                self.locals.push(ConstLocal::new(local.id(), name.clone()));
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                let value = ty
                    .as_ref()
                    .and_then(|ty| self.const_kind_for_type(ty))
                    .map(|kind| {
                        ConstExpr::unknown(kind, ty.as_ref().map(|ty| ty.span()).unwrap_or(*span))
                    })
                    .unwrap_or_else(|| {
                        self.unsupported = true;
                        ConstExpr::unsupported(*span)
                    });
                self.push_block(
                    vec![ConstStmt::Assign { local, value }],
                    Terminator::Goto(rest),
                )
            }
            HirStmt::Expr(expr) => {
                let rest = self.lower_stmt_suffix(stmts, index + 1, next);
                if let Some((local, value)) = self.lower_assignment(expr) {
                    self.push_block(
                        vec![ConstStmt::Assign { local, value }],
                        Terminator::Goto(rest),
                    )
                } else {
                    self.unsupported = true;
                    rest
                }
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
                let cond = self.lower_expr(cond);
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
                let cond = self.lower_expr(cond);
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
                let value = value.as_ref().map(|expr| self.lower_expr(expr));
                self.push_block(Vec::new(), Terminator::Return(value))
            }
            _ => {
                self.unsupported = true;
                self.lower_stmt_suffix(stmts, index + 1, next)
            }
        }
    }

    fn lower_block_to(&mut self, block: &HirBlock, next: BlockId) -> BlockId {
        let next = if let Some(tail) = &block.tail {
            if let Some((local, value)) = self.lower_assignment(tail) {
                self.push_block(
                    vec![ConstStmt::Assign { local, value }],
                    Terminator::Goto(next),
                )
            } else {
                self.unsupported = true;
                next
            }
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

    fn lower_assignment(&mut self, expr: &HirBodyExpr) -> Option<(ConstLocalRef, ConstExpr)> {
        let HirExprNode::Binary { op, left, right } = &expr.node else {
            return None;
        };
        if !matches!(MirBinaryOp::from(*op), MirBinaryOp::Assign) {
            return None;
        }
        let HirExprNode::Ident(name) = &left.node else {
            return None;
        };
        Some((self.local_ref_for_expr(left, name), self.lower_expr(right)))
    }

    fn lower_expr(&mut self, expr: &HirBodyExpr) -> ConstExpr {
        match &expr.node {
            HirExprNode::Ident(name) => {
                let const_value = self
                    .owner
                    .tir
                    .hir()
                    .expr_resolution(self.def, expr)
                    .ok()
                    .flatten()
                    .and_then(|resolution| match resolution {
                        HirResolution::Def(def) => self.owner.tir.hir().const_by_def(def),
                        HirResolution::Local(_) => None,
                        _ => None,
                    });
                const_value
                    .map(|item| self.lower_expr(&item.value))
                    .unwrap_or_else(|| {
                        ConstExpr::local(self.local_ref_for_expr(expr, name), expr.span())
                    })
            }
            HirExprNode::Int(value) => ConstExpr::int(*value, expr.span()),
            HirExprNode::Bool(value) => ConstExpr::bool_value(*value, expr.span()),
            HirExprNode::Group(inner) => self.lower_expr(inner),
            HirExprNode::Unary {
                op, expr: inner, ..
            } => {
                let op = MirUnaryOp::from(*op);
                if matches!(op, MirUnaryOp::Unsupported) {
                    return self.unsupported_expr(expr.span());
                }
                ConstExpr::unary(op, self.lower_expr(inner), expr.span())
            }
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                let op = MirBinaryOp::from(*op);
                if matches!(
                    op,
                    MirBinaryOp::Assign | MirBinaryOp::Field | MirBinaryOp::Unsupported
                ) {
                    return self.unsupported_expr(expr.span());
                }
                ConstExpr::binary(
                    op,
                    self.lower_expr(left),
                    self.lower_expr(right),
                    expr.span(),
                )
            }
            HirExprNode::Call { callee, args } => {
                let Some(root) = self.callee_root(callee) else {
                    return self.unsupported_expr(expr.span());
                };
                let Ok(Some(HirResolution::Def(def))) =
                    self.owner.tir.hir().expr_resolution(self.def, root)
                else {
                    return self.unsupported_expr(expr.span());
                };
                if !self.owner.tir.hir().fns.contains_key(&def) {
                    return self.unsupported_expr(expr.span());
                }
                ConstExpr::call(
                    def,
                    args.iter().map(|arg| self.lower_expr(&arg.value)).collect(),
                    expr.span(),
                )
            }
            HirExprNode::GenericApp { callee, .. } => self.lower_expr(callee),
            HirExprNode::Unsupported => self.unsupported_expr(expr.span()),
            _ => self.unsupported_expr(expr.span()),
        }
    }

    fn unsupported_expr(&mut self, span: Span) -> ConstExpr {
        self.unsupported = true;
        if self.unsupported_span.is_none() {
            self.unsupported_span = Some(span);
        }
        ConstExpr::unsupported(span)
    }

    fn callee_root<'c>(&self, expr: &'c HirBodyExpr) -> Option<&'c HirBodyExpr> {
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

    fn local_ref_for_decl(&self, id: Option<LocalId>, name: &str) -> ConstLocalRef {
        ConstLocalRef::new(id, name.to_string())
    }

    fn local_ref_for_expr(&self, expr: &HirBodyExpr, name: &str) -> ConstLocalRef {
        let id = self
            .owner
            .tir
            .hir()
            .expr_resolution(self.def, expr)
            .ok()
            .flatten()
            .and_then(|resolution| match resolution {
                HirResolution::Local(id) => Some(id),
                HirResolution::Def(_) => None,
                _ => None,
            });
        ConstLocalRef::new(id, name.to_string())
    }

    fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
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
}
