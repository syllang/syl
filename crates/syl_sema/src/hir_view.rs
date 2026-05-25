use crate::{
    hir::{HirCallable, HirConstItem, HirDefKind, HirDesign},
    hir_resolve::HirResolution,
    mir::MirTypeRef,
};
use syl_hir::{DefId, ExprId, name::HirPath};

pub(crate) trait HirDesignViewExt {
    fn expr_resolution(
        &self,
        owner: DefId,
        expr: &crate::hir::HirBodyExpr,
    ) -> Result<Option<HirResolution>, crate::CompileError>;

    fn register_expr_resolution(&mut self, id: ExprId, resolution: HirResolution);

    fn resolve_def_id(&self, owner: DefId, name: &str) -> Option<DefId>;

    fn def_kind(&self, id: DefId) -> Option<HirDefKind>;

    fn callable_by_def(&self, id: DefId) -> Option<&HirCallable>;

    fn const_by_def(&self, id: DefId) -> Option<&HirConstItem>;

    fn member_field_type(
        &self,
        type_def: DefId,
        view: Option<&str>,
        field: &str,
    ) -> Option<MirTypeRef>;

    fn expr_id(&self, owner: DefId, expr: &crate::hir::HirBodyExpr) -> Option<ExprId>;
}

impl HirDesignViewExt for HirDesign {
    fn expr_resolution(
        &self,
        _owner: DefId,
        expr: &crate::hir::HirBodyExpr,
    ) -> Result<Option<HirResolution>, crate::CompileError> {
        Ok(self.expr_resolutions.get(&expr.id()).copied())
    }

    fn register_expr_resolution(&mut self, id: ExprId, resolution: HirResolution) {
        self.expr_resolutions.insert(id, resolution);
    }

    fn resolve_def_id(&self, owner: DefId, name: &str) -> Option<DefId> {
        let package = self
            .defs
            .get(owner.get())
            .map(|def| def.canonical_path.parent())?;
        if let Some(def) = self
            .canonical_def_names
            .get(&package.with_leaf(name))
            .copied()
        {
            return Some(def);
        }
        let mut imported = self
            .imports
            .iter()
            .filter(|import| import.package_path == package)
            .filter(|import| import.path.last().is_some_and(|leaf| leaf == name))
            .filter_map(|import| {
                self.canonical_def_names
                    .get(&HirPath::new(import.path.clone()))
            })
            .copied();
        let first = imported.next()?;
        if imported.next().is_some() {
            return None;
        }
        Some(first)
    }

    fn def_kind(&self, id: DefId) -> Option<HirDefKind> {
        self.defs.get(id.get()).map(|def| def.kind)
    }

    fn callable_by_def(&self, id: DefId) -> Option<&HirCallable> {
        self.callables.get(&id)
    }

    fn const_by_def(&self, id: DefId) -> Option<&HirConstItem> {
        self.consts.get(&id)
    }

    fn member_field_type(
        &self,
        type_def: DefId,
        _view: Option<&str>,
        field: &str,
    ) -> Option<MirTypeRef> {
        self.bundles
            .get(&type_def)
            .and_then(|item| item.fields.iter().find(|decl| decl.name == field))
            .map(|decl| decl.ty.clone())
            .or_else(|| {
                self.interfaces
                    .get(&type_def)
                    .and_then(|item| item.fields.iter().find(|decl| decl.name == field))
                    .map(|decl| decl.ty.clone())
            })
            .or_else(|| {
                self.maps
                    .get(&type_def)
                    .and_then(|item| item.ret_ty.as_ref())
                    .map(|ret_ty| ret_ty.ty.clone())
            })
    }

    fn expr_id(&self, owner: DefId, expr: &crate::hir::HirBodyExpr) -> Option<ExprId> {
        if let Some(registered) = self.exprs.get(expr.id().get())
            && registered.owner == owner
            && registered.span == expr.span()
        {
            return Some(expr.id());
        }
        self.exprs
            .iter()
            .find(|registered| registered.owner == owner && registered.span == expr.span())
            .map(|expr| expr.id)
    }
}
