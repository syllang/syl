use crate::{
    eir::EirExpr,
    mir::{MirConstExpr, MirTypeRef},
    program::{ElabCallArg, ElabCallableItem, ElabExpr, ElabPortDirection},
};
use syl_hir::DefId;
use syl_span::Span;

#[non_exhaustive]
pub(crate) struct PortSpec<'a> {
    pub(crate) doc: Option<&'a str>,
    pub(crate) name: &'a str,
    pub(crate) dir: ElabPortDirection,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct ViewPortSpec<'a> {
    pub(crate) doc: Option<&'a str>,
    pub(crate) name: &'a str,
    pub(crate) base: &'a MirTypeRef,
    pub(crate) view: &'a str,
    pub(crate) array_len: Option<&'a MirConstExpr>,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct ViewSignalSpec<'a> {
    pub(crate) binding: &'a str,
    pub(crate) physical_prefix: &'a str,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct InstanceEmitRequest<'a> {
    pub(crate) inst_name: &'a str,
    pub(crate) callee: &'a ElabExpr,
    pub(crate) args: &'a [ElabCallArg],
    pub(crate) env: &'a super::super::Env,
    pub(crate) inplace: bool,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct CellInlineRequest<'a> {
    pub(crate) callable_def: DefId,
    pub(crate) inst_name: &'a str,
    pub(crate) callable_name: &'a str,
    pub(crate) item: &'a ElabCallableItem,
    pub(crate) callee: &'a ElabExpr,
    pub(crate) args: &'a [ElabCallArg],
    pub(crate) caller_env: &'a super::super::Env,
}

#[non_exhaustive]
pub(crate) struct CellArgBindingRequest<'a> {
    pub(crate) formal: &'a str,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) actual: &'a ElabExpr,
    pub(crate) caller_env: &'a super::super::Env,
}

#[non_exhaustive]
pub(crate) struct ViewArgConnectionRequest<'a> {
    pub(crate) formal_owner: Option<DefId>,
    pub(crate) formal: &'a str,
    pub(crate) actual: &'a ElabExpr,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) env: &'a super::super::Env,
}

#[non_exhaustive]
pub(crate) struct ConnectionPushRequest<'a> {
    pub(crate) formal: &'a str,
    pub(crate) actual: EirExpr,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(crate) struct ResultConnectionRequest<'a> {
    pub(crate) formal_owner: Option<DefId>,
    pub(crate) formal: &'a str,
    pub(crate) actual: &'a str,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) span: Span,
}
