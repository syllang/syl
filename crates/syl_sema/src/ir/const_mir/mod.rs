use crate::{
    CompileError,
    hir::{
        HirBodyExpr, HirConstItem, HirDesign, HirEnumVariantKey, resolve::HirResolution,
        view::HirDesignViewExt,
    },
    ir::mir::{MirBinaryOp, MirTypeRef, MirUnaryOp},
    tir::{TirDesign, TirType},
};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId, LocalId};
use syl_span::Span;

mod builder;
mod eval;
mod lower;
mod metrics;

pub use eval::{ConstEvalEnv, ConstEvaluator};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ConstKind {
    Nat,
    Bool,
    Struct(ConstStructKind),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ConstStructKind {
    def: DefId,
}

impl ConstStructKind {
    fn new(def: DefId) -> Self {
        Self { def }
    }

    pub fn def(self) -> DefId {
        self.def
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ConstValue {
    Unknown(ConstKind),
    Nat(u64),
    Bool(bool),
    Struct(ConstStructValue),
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ConstStructValue {
    kind: ConstStructKind,
    fields: Vec<ConstStructFieldValue>,
}

impl ConstStructValue {
    pub fn new(kind: ConstStructKind, fields: Vec<ConstStructFieldValue>) -> Self {
        Self { kind, fields }
    }

    pub fn kind(&self) -> ConstStructKind {
        self.kind
    }

    pub fn fields(&self) -> &[ConstStructFieldValue] {
        &self.fields
    }

    pub fn field_value(&self, field: &str) -> Option<&ConstValue> {
        self.fields
            .iter()
            .find(|named| named.name == field)
            .map(|named| &named.value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ConstStructFieldValue {
    name: String,
    value: ConstValue,
}

impl ConstStructFieldValue {
    pub fn new(name: impl Into<String>, value: ConstValue) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &ConstValue {
        &self.value
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct ConstStructDef {
    kind: ConstStructKind,
    name: String,
    fields: Vec<ConstStructFieldDef>,
}

impl ConstStructDef {
    fn new(kind: ConstStructKind, name: String, fields: Vec<ConstStructFieldDef>) -> Self {
        Self { kind, name, fields }
    }

    pub fn kind(&self) -> ConstStructKind {
        self.kind
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[ConstStructFieldDef] {
        &self.fields
    }

    pub fn field(&self, field: &str) -> Option<&ConstStructFieldDef> {
        self.fields.iter().find(|named| named.name == field)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct ConstStructFieldDef {
    name: String,
    kind: Option<ConstKind>,
}

impl ConstStructFieldDef {
    fn new(name: impl Into<String>, kind: Option<ConstKind>) -> Self {
        Self {
            name: name.into(),
            kind,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> Option<ConstKind> {
        self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConstNamedExpr {
    name: String,
    value: ConstExpr,
}

impl ConstNamedExpr {
    pub fn new(name: impl Into<String>, value: ConstExpr) -> Self {
        Self {
            name: name.into(),
            value,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &ConstExpr {
        &self.value
    }
}

/// Boundary for const evaluation so the evaluator can be tested with tiny
/// function stores instead of a full `ConstMirProgram`.
trait ConstFunctionStore {
    fn function(&self, def: DefId) -> Option<&ConstFunction>;
}

/// Boundary for type-kind classification so callers can swap in fake
/// classification rules during tests without rewriting const evaluation.
trait ConstTypeOracle {
    fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind>;
}

pub(crate) trait ConstMirLoweringContext {
    fn hir(&self) -> &HirDesign;

    fn is_const_owner(&self, owner: DefId) -> bool;

    fn expr_resolution(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Option<HirResolution>, CompileError>;

    fn expr_type(&self, owner: DefId, expr: &HirBodyExpr) -> Option<&TirType>;

    fn const_by_def(&self, def: DefId) -> Option<&HirConstItem>;

    fn function_exists(&self, def: DefId) -> bool;

    fn extension_method_call<'a>(
        &self,
        owner: DefId,
        callee: &'a HirBodyExpr,
    ) -> Option<(DefId, &'a HirBodyExpr)>;

    fn enum_variant_value(&self, expr: &HirBodyExpr) -> Option<u64>;
}

#[non_exhaustive]
pub struct ConstMirProgram {
    functions: Vec<ConstFunction>,
    function_index: BTreeMap<DefId, usize>,
    structs: BTreeMap<DefId, ConstStructDef>,
    struct_path_index: BTreeMap<Vec<String>, DefId>,
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

    pub fn struct_def(&self, id: DefId) -> Option<&ConstStructDef> {
        self.structs.get(&id)
    }

    pub fn struct_kind(&self, id: DefId) -> Option<ConstStructKind> {
        self.structs.get(&id).map(ConstStructDef::kind)
    }

    pub fn field_kind(&self, kind: ConstStructKind, field: &str) -> Option<ConstKind> {
        self.structs
            .get(&kind.def())
            .and_then(|item| item.field(field))
            .and_then(ConstStructFieldDef::kind)
    }

    fn struct_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
        if let Some(path) = ty.path() {
            let def = self.struct_path_index.get(path).copied();
            return def.map(ConstStructKind::new).map(ConstKind::Struct);
        }
        if let Some(base) = ty.generic_base() {
            return self.struct_kind_for_type(base);
        }
        if let Some((base, _)) = ty.view_select() {
            return self.struct_kind_for_type(base);
        }
        if let Some((_, elem)) = ty.array() {
            return self.struct_kind_for_type(elem);
        }
        None
    }
}

impl ConstFunctionStore for ConstMirProgram {
    fn function(&self, def: DefId) -> Option<&ConstFunction> {
        ConstMirProgram::function(self, def)
    }
}

impl ConstTypeOracle for ConstMirProgram {
    fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
        match ty.type_name() {
            Some("nat") => Some(ConstKind::Nat),
            Some("bool") => Some(ConstKind::Bool),
            _ => self.struct_kind_for_type(ty),
        }
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

    fn expr_type(&self, owner: DefId, expr: &HirBodyExpr) -> Option<&TirType> {
        let id = match &expr.node {
            crate::hir::HirExprNode::Ident(name) => TirDesign::hir(self)
                .expr_resolution(owner, expr)
                .ok()
                .flatten()
                .and_then(|resolution| match resolution {
                    HirResolution::Local(id) => self
                        .binding_types()
                        .get(&crate::tir::BindingRef::Local(id))
                        .copied(),
                    HirResolution::Def(id) => self
                        .binding_types()
                        .get(&crate::tir::BindingRef::Def(id))
                        .copied(),
                    _ => None,
                })
                .or_else(|| {
                    TirDesign::hir(self)
                        .locals
                        .iter()
                        .filter(|local| {
                            local.owner == owner
                                && local.name == *name
                                && local.span.start <= expr.span().start
                        })
                        .max_by_key(|local| local.span.start)
                        .and_then(|local| {
                            self.binding_types()
                                .get(&crate::tir::BindingRef::Local(local.id))
                                .copied()
                        })
                })?,
            _ => *self.expr_types().get(&expr.id())?,
        };
        self.type_table().get(id)
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

#[derive(Clone, Debug, PartialEq, Eq)]
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

    pub fn aggregate(kind: ConstStructKind, fields: Vec<ConstNamedExpr>, span: Span) -> Self {
        Self::new(ConstExprKind::Aggregate { kind, fields }, span)
    }

    pub fn field(base: ConstExpr, field: impl Into<String>, span: Span) -> Self {
        Self::new(
            ConstExprKind::Field {
                base: Box::new(base),
                field: field.into(),
            },
            span,
        )
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConstExprKind {
    Local(ConstLocalRef),
    Unknown(ConstKind),
    Nat(u64),
    Bool(bool),
    Aggregate {
        kind: ConstStructKind,
        fields: Vec<ConstNamedExpr>,
    },
    Field {
        base: Box<ConstExpr>,
        field: String,
    },
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
