use crate::{
    hir::{HirDesign, HirInterfaceItem},
    hir_view::HirDesignViewExt,
    mir::MirTypeRef,
};
use syl_hir::DefId;
use syl_hir::name::HirPath;

#[non_exhaustive]
pub(crate) struct MirTypeDefinitionResolver<'a> {
    design: &'a HirDesign,
}

impl<'a> MirTypeDefinitionResolver<'a> {
    pub(crate) fn new(design: &'a HirDesign) -> Self {
        Self { design }
    }

    pub(crate) fn def_id(&self, owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
        self.def_id_structural(owner, ty)
    }

    pub(crate) fn interface(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
    ) -> Option<&'a HirInterfaceItem> {
        let def = self.def_id(owner?, ty)?;
        self.design.interfaces.get(&def)
    }

    pub(crate) fn type_name_or_unknown(&self, ty: &MirTypeRef) -> String {
        ty.type_name()
            .map(str::to_string)
            .unwrap_or_else(|| "<unknown>".to_string())
    }

    fn def_id_structural(&self, owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
        if let Some(path) = ty.path() {
            return self.path_def_id(owner, path);
        }
        if let Some((base, _)) = ty.view_select() {
            return self.def_id_structural(owner, base);
        }
        if let Some(base) = ty.generic_base() {
            return self.def_id_structural(owner, base);
        }
        if let Some((_, elem)) = ty.array() {
            return self.def_id_structural(owner, elem);
        }
        None
    }

    fn path_def_id(&self, owner: DefId, path: &[String]) -> Option<DefId> {
        if path.len() == 1 {
            return self.design.resolve_def_id(owner, &path[0]);
        }
        self.design
            .canonical_def_names
            .get(&HirPath::new(path.to_vec()))
            .copied()
    }
}
