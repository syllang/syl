use crate::{
    mir::MirTypeRef,
    program::{ElabBlock, ElabCallArg, ElabExpr, ElabNamedExpr, ElabRegReset},
};
use syl_hir::LocalId;
use syl_span::Span;

#[non_exhaustive]
pub(crate) struct SignalEmit<'a> {
    pub(crate) name: &'a str,
    pub(crate) ty: Option<MirTypeRef>,
    pub(crate) value: Option<&'a ElabExpr>,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct ConstEmit<'a> {
    pub(crate) name: &'a str,
    pub(crate) ty: Option<MirTypeRef>,
    pub(crate) value: &'a ElabExpr,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct RegEmit<'a> {
    pub(crate) name: &'a str,
    pub(crate) ty: Option<MirTypeRef>,
    pub(crate) reset: Option<&'a ElabRegReset>,
    pub(crate) span: Span,
    pub(crate) body: &'a ElabBlock,
}

#[non_exhaustive]
pub(crate) struct IfEmit<'a> {
    pub(crate) cond: &'a ElabExpr,
    pub(crate) then_block: &'a ElabBlock,
    pub(crate) else_block: Option<&'a ElabBlock>,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct LetPlaceEmit<'a> {
    pub(crate) name: &'a str,
    pub(crate) callee: &'a ElabExpr,
    pub(crate) args: &'a [ElabCallArg],
    pub(crate) inplace: bool,
    pub(crate) value: &'a ElabExpr,
}

#[non_exhaustive]
pub(crate) struct ForLetEmit<'a> {
    pub(crate) binding_name: &'a str,
    pub(crate) loop_name: &'a str,
    pub(crate) range: &'a ElabExpr,
    pub(crate) body: &'a ElabBlock,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct BindVarEmit<'a> {
    pub(crate) id: Option<LocalId>,
    pub(crate) name: &'a str,
    pub(crate) ty: Option<&'a MirTypeRef>,
    pub(crate) value: Option<&'a ElabExpr>,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct AggregateAssignEmit<'a> {
    pub(crate) target: &'a ElabExpr,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) fields: &'a [ElabNamedExpr],
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct ForEmit<'a> {
    pub(crate) name: &'a str,
    pub(crate) range_expr: &'a ElabExpr,
    pub(crate) body: &'a ElabBlock,
    pub(crate) span: Span,
}
