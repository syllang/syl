use crate::{
    eir_build::Env,
    eir_expr::EirExpr,
    mir::{MirConstExpr, MirTypeRef},
    program::{ElabCallArg, ElabCallableItem, ElabExpr, ElabPortDirection},
};
use syl_hir::DefId;
use syl_span::Span;

#[non_exhaustive]
pub(crate) struct PortSpec<'a> {
    pub(crate) name: &'a str,
    pub(crate) dir: ElabPortDirection,
    pub(crate) ty: &'a MirTypeRef,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(super) struct ViewPortSpec<'a> {
    pub(super) name: &'a str,
    pub(super) base: &'a MirTypeRef,
    pub(super) view: &'a str,
    pub(super) array_len: Option<&'a MirConstExpr>,
    pub(super) span: Span,
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
    pub(crate) env: &'a Env,
    pub(crate) inplace: bool,
    pub(crate) span: Span,
}

#[non_exhaustive]
pub(super) struct CellInlineRequest<'a> {
    pub(super) callable_def: DefId,
    pub(super) inst_name: &'a str,
    pub(super) callable_name: &'a str,
    pub(super) item: &'a ElabCallableItem,
    pub(super) callee: &'a ElabExpr,
    pub(super) args: &'a [ElabCallArg],
    pub(super) caller_env: &'a Env,
}

#[non_exhaustive]
pub(super) struct CellArgBindingRequest<'a> {
    pub(super) formal: &'a str,
    pub(super) ty: &'a MirTypeRef,
    pub(super) actual: &'a ElabExpr,
    pub(super) caller_env: &'a Env,
}

#[non_exhaustive]
pub(super) struct ViewArgConnectionRequest<'a> {
    pub(super) formal_owner: Option<DefId>,
    pub(super) formal: &'a str,
    pub(super) actual: &'a ElabExpr,
    pub(super) ty: &'a MirTypeRef,
    pub(super) env: &'a Env,
}

#[non_exhaustive]
pub(super) struct ConnectionPushRequest<'a> {
    pub(super) formal: &'a str,
    pub(super) actual: EirExpr,
    pub(super) span: Span,
}

#[non_exhaustive]
pub(super) struct ResultConnectionRequest<'a> {
    pub(super) formal_owner: Option<DefId>,
    pub(super) formal: &'a str,
    pub(super) actual: &'a str,
    pub(super) ty: &'a MirTypeRef,
    pub(super) span: Span,
}
