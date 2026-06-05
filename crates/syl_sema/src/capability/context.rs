use super::{
    model::{EndpointSide, FieldCaps},
    place::{PlaceResolution, PlaceResolver},
    view::ViewCapabilityResolver,
};
use crate::{
    CompileError,
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{HirBodyExpr, HirCallable, HirDefKind, HirDesign},
    ir::{mir::MirTypeRef, mir::MirTypeRefExt, mir_type_resolve::MirTypeDefinitionResolver},
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hir::DefId;

pub(super) trait CapabilityContext {
    fn callables(&self) -> &BTreeMap<DefId, HirCallable>;

    fn resolve_place(&self, owner: DefId, expr: &HirBodyExpr) -> PlaceResolution;

    fn expr_resolution(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Option<HirResolution>, CompileError>;

    fn def_kind(&self, def: DefId) -> Option<HirDefKind>;

    fn def_name(&self, def: DefId) -> Option<&str>;

    fn callable_by_def(&self, def: DefId) -> Option<&HirCallable>;

    fn view_caps(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
        side: EndpointSide,
    ) -> Result<Option<FieldCaps>, CompileError>;

    fn view_field_names(&self, owner: DefId, ty: &MirTypeRef) -> Option<BTreeSet<String>>;
}

impl CapabilityContext for HirDesign {
    fn callables(&self) -> &BTreeMap<DefId, HirCallable> {
        &self.callables
    }

    fn resolve_place(&self, owner: DefId, expr: &HirBodyExpr) -> PlaceResolution {
        PlaceResolver::new(self, owner, expr).resolve()
    }

    fn expr_resolution(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Option<HirResolution>, CompileError> {
        HirDesignViewExt::expr_resolution(self, owner, expr)
    }

    fn def_kind(&self, def: DefId) -> Option<HirDefKind> {
        HirDesignViewExt::def_kind(self, def)
    }

    fn def_name(&self, def: DefId) -> Option<&str> {
        HirDesign::def_name(self, def)
    }

    fn callable_by_def(&self, def: DefId) -> Option<&HirCallable> {
        HirDesignViewExt::callable_by_def(self, def)
    }

    fn view_caps(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
        side: EndpointSide,
    ) -> Result<Option<FieldCaps>, CompileError> {
        ViewCapabilityResolver::new(self).caps(owner, ty, side)
    }

    fn view_field_names(&self, owner: DefId, ty: &MirTypeRef) -> Option<BTreeSet<String>> {
        let (base, view, _) = ty.view_shape()?;
        let resolver = MirTypeDefinitionResolver::new(self);
        let interface = resolver.interface(Some(owner), base)?;
        let view_decl = interface.views.iter().find(|decl| decl.name == view)?;
        Some(
            view_decl
                .fields
                .iter()
                .map(|field| field.name.clone())
                .collect(),
        )
    }
}
