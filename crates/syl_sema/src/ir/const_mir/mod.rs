use crate::{
    CompileError,
    hir::{
        HirBlock, HirBodyExpr, HirConstItem, HirDesign, HirEnumVariantKey, HirFnItem, HirStmt,
        resolve::HirResolution, view::HirDesignViewExt,
    },
    ir::mir::{MirBinaryOp, MirUnaryOp},
    tir::TirDesign,
};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId, LocalId};
use syl_span::Span;

mod eval;
mod lower;
mod metrics;

use lower::ExprLowerer;

pub use eval::{ConstEvalEnv, ConstEvaluator, ConstKind, ConstValue};

/// Boundary for const evaluation so the evaluator can be tested with tiny
/// function stores instead of a full `ConstMirProgram`.
trait ConstFunctionStore {
    fn function(&self, def: DefId) -> Option<&ConstFunction>;
}

/// Boundary for type-kind classification so callers can swap in fake
/// classification rules during tests without rewriting const evaluation.
trait ConstTypeOracle {
    fn const_kind_for_type(&self, ty: &crate::ir::mir::MirTypeRef) -> Option<ConstKind>;
}

pub(crate) trait ConstMirLoweringContext {
    fn hir(&self) -> &HirDesign;

    fn is_const_owner(&self, owner: DefId) -> bool;

    fn expr_resolution(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Option<HirResolution>, CompileError>;

    fn const_by_def(&self, def: DefId) -> Option<&HirConstItem>;

    fn function_exists(&self, def: DefId) -> bool;

    fn extension_method_call<'a>(
        &self,
        owner: DefId,
        callee: &'a HirBodyExpr,
    ) -> Option<(DefId, &'a HirBodyExpr)>;

    fn enum_variant_value(&self, expr: &HirBodyExpr) -> Option<u64>;
}

struct BuiltinConstTypeOracle;

impl ConstTypeOracle for BuiltinConstTypeOracle {
    fn const_kind_for_type(&self, ty: &crate::ir::mir::MirTypeRef) -> Option<ConstKind> {
        match ty.type_name() {
            Some("Nat") => Some(ConstKind::Nat),
            Some("Bool") => Some(ConstKind::Bool),
            _ => None,
        }
    }
}

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

impl ConstFunctionStore for ConstMirProgram {
    fn function(&self, def: DefId) -> Option<&ConstFunction> {
        ConstMirProgram::function(self, def)
    }
}

impl ConstMirLoweringContext for TirDesign {
    fn hir(&self) -> &HirDesign {
        TirDesign::hir(self)
    }

    fn is_const_owner(&self, owner: DefId) -> bool {
        TirDesign::hir(self).consts.contains_key(&owner)
    }

    fn expr_resolution(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Option<HirResolution>, CompileError> {
        TirDesign::hir(self).expr_resolution(owner, expr)
    }

    fn const_by_def(&self, def: DefId) -> Option<&HirConstItem> {
        TirDesign::hir(self).consts.get(&def)
    }

    fn function_exists(&self, def: DefId) -> bool {
        TirDesign::hir(self).fns.contains_key(&def)
    }

    fn extension_method_call<'a>(
        &self,
        owner: DefId,
        callee: &'a HirBodyExpr,
    ) -> Option<(DefId, &'a HirBodyExpr)> {
        TirDesign::extension_method_call(self, owner, callee)
            .map(|call| (call.method, call.receiver))
    }

    fn enum_variant_value(&self, expr: &HirBodyExpr) -> Option<u64> {
        let (enum_def, variant) = TirDesign::hir(self).enum_variant_expr(expr)?;
        self.enum_variant_values()
            .get(&HirEnumVariantKey::new(enum_def, variant))
            .copied()
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
    origin: Option<ExprId>,
}

impl ConstExpr {
    fn new(kind: ConstExprKind, span: Span) -> Self {
        Self {
            kind,
            span,
            origin: None,
        }
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

    pub fn nat(value: u64, span: Span) -> Self {
        Self::new(ConstExprKind::Nat(value), span)
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

    pub fn origin(&self) -> Option<ExprId> {
        self.origin
    }

    pub fn with_origin(mut self, origin: ExprId) -> Self {
        self.origin = Some(origin);
        self
    }
}

#[non_exhaustive]
pub enum ConstExprKind {
    Local(ConstLocalRef),
    Unknown(ConstKind),
    Nat(u64),
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
    ctx: &'a dyn ConstMirLoweringContext,
}

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
        Ok(ConstMirProgram {
            functions,
            function_index,
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
