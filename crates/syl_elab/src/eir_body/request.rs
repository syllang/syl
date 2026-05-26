use crate::{
    mir::MirTypeRef,
    program::{ElabBlock, ElabCallArg, ElabExpr, ElabNamedExpr, ElabRegReset},
};
use syl_span::Span;

#[non_exhaustive]
pub(super) struct SignalEmit<'a> {
    pub(super) name: &'a str,
    pub(super) ty: Option<MirTypeRef>,
    pub(super) value: Option<&'a ElabExpr>,
    pub(super) span: Span,
}

#[non_exhaustive]
pub(super) struct ConstEmit<'a> {
    pub(super) name: &'a str,
    pub(super) ty: Option<MirTypeRef>,
    pub(super) value: &'a ElabExpr,
    pub(super) span: Span,
}

#[non_exhaustive]
pub(super) struct RegEmit<'a> {
    pub(super) name: &'a str,
    pub(super) ty: Option<MirTypeRef>,
    pub(super) reset: Option<&'a ElabRegReset>,
    pub(super) span: Span,
    pub(super) body: &'a ElabBlock,
}

#[non_exhaustive]
pub(super) struct IfEmit<'a> {
    pub(super) cond: &'a ElabExpr,
    pub(super) then_block: &'a ElabBlock,
    pub(super) else_block: Option<&'a ElabBlock>,
    pub(super) span: Span,
}

#[non_exhaustive]
pub(super) struct LetPlaceEmit<'a> {
    pub(super) name: &'a str,
    pub(super) callee: &'a ElabExpr,
    pub(super) args: &'a [ElabCallArg],
    pub(super) inplace: bool,
    pub(super) value: &'a ElabExpr,
}

#[non_exhaustive]
pub(super) struct AggregateAssignEmit<'a> {
    pub(super) target: &'a ElabExpr,
    pub(super) ty: &'a MirTypeRef,
    pub(super) fields: &'a [ElabNamedExpr],
    pub(super) span: Span,
}

#[non_exhaustive]
pub(super) struct ForEmit<'a> {
    pub(super) name: &'a str,
    pub(super) range_expr: &'a ElabExpr,
    pub(super) body: &'a ElabBlock,
    pub(super) span: Span,
}
