use crate::{
    source::{HirDefKind, HirLocalKind, HirPortDirection, HirViewDirection},
    tir::{BindingRef, TirDesign, TirType},
};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId, HirExtensionMethodIndex, HirPath, HirResolution, LocalId};

mod body;
mod item;

pub(crate) use body::{
    ElabBlock, ElabCallArg, ElabExpr, ElabExprNode, ElabMatchArm, ElabNamedExpr, ElabRegReset,
    ElabSelectArm, ElabStmt,
};
use item::ElabEnumVariantKey;
pub(crate) use item::{
    ElabBundleItem, ElabCallable, ElabCallableItem, ElabConstItem, ElabEnumItem,
    ElabExternCellItem, ElabInterfaceItem, ElabSignatureGenericParam, ElabSignatureResultBinding,
};

#[non_exhaustive]
pub(crate) struct ElabProgram {
    defs: Vec<ElabDef>,
    canonical_paths: BTreeMap<DefId, HirPath>,
    visible_defs: BTreeMap<(DefId, String), DefId>,
    canonical_defs: BTreeMap<HirPath, DefId>,
    expr_resolutions_by_id: BTreeMap<(DefId, ExprId), ElabResolution>,
    extension_methods: HirExtensionMethodIndex,
    expr_types: BTreeMap<(DefId, ExprId), TirType>,
    local_types: BTreeMap<LocalId, TirType>,
    local_kinds: BTreeMap<LocalId, ElabLocalKind>,
    consts: BTreeMap<DefId, ElabConstItem>,
    enums: BTreeMap<DefId, ElabEnumItem>,
    enum_variants: BTreeMap<ElabEnumVariantKey, u64>,
    bundles: BTreeMap<DefId, ElabBundleItem>,
    interfaces: BTreeMap<DefId, ElabInterfaceItem>,
    callables: BTreeMap<DefId, ElabCallable>,
}

impl ElabProgram {
    pub(crate) fn from_tir(tir: &TirDesign) -> Self {
        ElabProgramBuilder::new(tir).build()
    }

    pub(crate) fn callables(&self) -> &BTreeMap<DefId, ElabCallable> {
        &self.callables
    }

    pub(crate) fn callable_by_def(&self, def: DefId) -> Option<&ElabCallable> {
        self.callables.get(&def)
    }

    pub(crate) fn const_by_def(&self, def: DefId) -> Option<&ElabConstItem> {
        self.consts.get(&def)
    }

    pub(crate) fn enum_by_def(&self, def: DefId) -> Option<&ElabEnumItem> {
        self.enums.get(&def)
    }

    pub(crate) fn bundle_by_def(&self, def: DefId) -> Option<&ElabBundleItem> {
        self.bundles.get(&def)
    }

    pub(crate) fn interface_by_def(&self, def: DefId) -> Option<&ElabInterfaceItem> {
        self.interfaces.get(&def)
    }

    pub(crate) fn def_name(&self, id: DefId) -> Option<&str> {
        self.defs.get(id.get()).map(|def| def.name.as_str())
    }

    pub(crate) fn def_kind(&self, id: DefId) -> Option<ElabDefKind> {
        self.defs.get(id.get()).map(|def| def.kind)
    }

    pub(crate) fn canonical_path(&self, id: DefId) -> Option<&HirPath> {
        self.canonical_paths.get(&id)
    }

    pub(crate) fn resolve_def_id(&self, owner: DefId, name: &str) -> Option<DefId> {
        self.visible_defs.get(&(owner, name.to_string())).copied()
    }

    pub(crate) fn canonical_def_id(&self, path: &[String]) -> Option<DefId> {
        self.canonical_defs
            .get(&HirPath::new(path.to_vec()))
            .copied()
    }

    pub(crate) fn expr_resolution(&self, owner: DefId, expr: &ElabExpr) -> Option<ElabResolution> {
        self.expr_resolutions_by_id
            .get(&(owner, expr.id()))
            .copied()
    }

    pub(crate) fn extension_methods_for(&self, receiver: DefId, name: &str) -> &[DefId] {
        self.extension_methods.methods_for(receiver, name)
    }

    pub(crate) fn expr_type(&self, owner: DefId, expr: &ElabExpr) -> Option<&TirType> {
        self.expr_types.get(&(owner, expr.id()))
    }

    pub(crate) fn local_type(&self, local: LocalId) -> Option<&TirType> {
        self.local_types.get(&local)
    }

    pub(crate) fn local_kind(&self, local: LocalId) -> Option<ElabLocalKind> {
        self.local_kinds.get(&local).copied()
    }

    pub(crate) fn enum_variant_value(&self, owner: DefId, path: &[String]) -> Option<u64> {
        let (variant, enum_path) = path.split_last()?;
        let enum_def = if enum_path.is_empty() {
            self.resolve_def_id(owner, variant)
        } else if enum_path.len() == 1 {
            self.resolve_def_id(owner, &enum_path[0])
        } else {
            self.canonical_def_id(enum_path)
        };
        enum_def.and_then(|def| self.variant_value(def, variant))
    }

    pub(crate) fn enum_variant_value_by_name(
        &self,
        owner: Option<DefId>,
        name: &str,
    ) -> Option<u64> {
        let owner = owner?;
        self.resolve_def_id(owner, name)
            .and_then(|def| self.variant_value(def, name))
    }

    pub(crate) fn enum_variant_field_value(
        &self,
        owner: DefId,
        base: &ElabExpr,
        field: &str,
    ) -> Option<u64> {
        let enum_def = self.enum_variant_base_def(owner, base)?;
        self.variant_value(enum_def, field)
    }

    fn variant_value(&self, enum_def: DefId, name: &str) -> Option<u64> {
        self.enum_variants
            .get(&ElabEnumVariantKey::new(enum_def, name))
            .copied()
    }

    fn enum_variant_base_def(&self, owner: DefId, expr: &ElabExpr) -> Option<DefId> {
        let mut current = expr;
        loop {
            match &current.node {
                ElabExprNode::Group(inner) => current = inner,
                ElabExprNode::Ident(_) => break,
                _ => return None,
            }
        }
        let ElabResolution::Def(def) = self.expr_resolution(owner, current)? else {
            return None;
        };
        (self.def_kind(def) == Some(ElabDefKind::Enum)).then_some(def)
    }

}

#[non_exhaustive]
struct ElabProgramBuilder<'a> {
    tir: &'a TirDesign,
}

impl<'a> ElabProgramBuilder<'a> {
    fn new(tir: &'a TirDesign) -> Self {
        Self { tir }
    }

    fn build(&self) -> ElabProgram {
        let hir = self.tir.hir();
        let enum_max_values = self
            .tir
            .enum_variant_values()
            .iter()
            .fold(BTreeMap::new(), |mut max_values, (key, value)| {
                max_values
                    .entry(key.enum_def)
                    .and_modify(|current| {
                        if *current < *value {
                            *current = *value;
                        }
                    })
                    .or_insert(*value);
                max_values
            });
        let mut visible_defs = BTreeMap::new();
        for owner in &hir.defs {
            for def in hir.visible_def_ids(owner.id) {
                if let Some(name) = hir.def_name(def) {
                    visible_defs.insert((owner.id, name.to_string()), def);
                }
            }
        }
        let expr_resolutions_by_id = hir
            .exprs
            .iter()
            .filter_map(|expr| {
                hir.expr_resolutions
                    .get(&expr.id)
                    .copied()
                    .map(|resolution| ((expr.owner, expr.id), ElabResolution::from(resolution)))
            })
            .collect();
        let local_types = self
            .tir
            .binding_types()
            .iter()
            .filter_map(|(binding, ty)| {
                let BindingRef::Local(local) = binding else {
                    return None;
                };
                self.tir
                    .type_table()
                    .get(*ty)
                    .cloned()
                    .map(|ty| (*local, ty))
            })
            .collect();
        let expr_types = hir
            .exprs
            .iter()
            .filter_map(|expr| {
                self.tir
                    .expr_types()
                    .get(&expr.id)
                    .and_then(|ty| self.tir.type_table().get(*ty))
                    .cloned()
                    .map(|ty| ((expr.owner, expr.id), ty))
            })
            .collect();
        ElabProgram {
            defs: hir
                .defs
                .iter()
                .map(|def| ElabDef::new(def.name.clone(), ElabDefKind::from(def.kind)))
                .collect(),
            canonical_paths: hir
                .defs
                .iter()
                .map(|def| (def.id, def.canonical_path.clone()))
                .collect(),
            visible_defs,
            canonical_defs: hir.canonical_def_names.clone(),
            expr_resolutions_by_id,
            extension_methods: hir.extension_methods.clone(),
            expr_types,
            local_types,
            local_kinds: hir
                .locals
                .iter()
                .map(|local| (local.id, ElabLocalKind::from(local.kind)))
                .collect(),
            consts: hir
                .consts
                .iter()
                .map(|(def, item)| (*def, ElabConstItem::from(item)))
                .collect(),
            enums: hir
                .enums
                .iter()
                .map(|(def, item)| {
                    (
                        *def,
                        ElabEnumItem::new(
                            item,
                            enum_max_values.get(def).copied().unwrap_or(0),
                        ),
                    )
                })
                .collect(),
            enum_variants: self
                .tir
                .enum_variant_values()
                .iter()
                .map(|(key, variant)| (ElabEnumVariantKey::from(key), *variant))
                .collect(),
            bundles: hir
                .bundles
                .iter()
                .map(|(def, item)| (*def, ElabBundleItem::from(item)))
                .collect(),
            interfaces: hir
                .interfaces
                .iter()
                .map(|(def, item)| (*def, ElabInterfaceItem::from(item)))
                .collect(),
            callables: hir
                .callables
                .iter()
                .map(|(def, item)| (*def, ElabCallable::from(item)))
                .collect(),
        }
    }
}

#[non_exhaustive]
struct ElabDef {
    name: String,
    kind: ElabDefKind,
}

impl ElabDef {
    fn new(name: String, kind: ElabDefKind) -> Self {
        Self { name, kind }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum ElabDefKind {
    Const,
    Fn,
    Enum,
    Bundle,
    Interface,
    Map,
    Cell,
    ExternCell,
    Unsupported,
}

impl From<HirDefKind> for ElabDefKind {
    fn from(value: HirDefKind) -> Self {
        match value {
            HirDefKind::Const => Self::Const,
            HirDefKind::Fn => Self::Fn,
            HirDefKind::Enum => Self::Enum,
            HirDefKind::Bundle => Self::Bundle,
            HirDefKind::Interface => Self::Interface,
            HirDefKind::Map => Self::Map,
            HirDefKind::Cell => Self::Cell,
            HirDefKind::ExternCell => Self::ExternCell,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum ElabResolution {
    Def(DefId),
    Local(LocalId),
    Unsupported,
}

impl From<HirResolution> for ElabResolution {
    fn from(value: HirResolution) -> Self {
        match value {
            HirResolution::Def(def) => Self::Def(def),
            HirResolution::Local(local) => Self::Local(local),
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum ElabLocalKind {
    Generic,
    Param,
    Result,
    Const,
    Let,
    Var,
    Signal,
    Reg,
    Instance,
    Loop,
    Unsupported,
}

impl From<HirLocalKind> for ElabLocalKind {
    fn from(value: HirLocalKind) -> Self {
        match value {
            HirLocalKind::Generic => Self::Generic,
            HirLocalKind::Param => Self::Param,
            HirLocalKind::Result => Self::Result,
            HirLocalKind::Const => Self::Const,
            HirLocalKind::Let => Self::Let,
            HirLocalKind::Var => Self::Var,
            HirLocalKind::Signal => Self::Signal,
            HirLocalKind::Reg => Self::Reg,
            HirLocalKind::Instance => Self::Instance,
            HirLocalKind::Loop => Self::Loop,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum ElabPortDirection {
    In,
    InOut,
    Out,
    Unsupported,
}

impl From<HirPortDirection> for ElabPortDirection {
    fn from(value: HirPortDirection) -> Self {
        match value {
            HirPortDirection::In => Self::In,
            HirPortDirection::InOut => Self::InOut,
            HirPortDirection::Out => Self::Out,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum ElabViewDirection {
    In,
    InOut,
    Out,
    Unsupported,
}

impl From<HirViewDirection> for ElabViewDirection {
    fn from(value: HirViewDirection) -> Self {
        match value {
            HirViewDirection::In => Self::In,
            HirViewDirection::InOut => Self::InOut,
            HirViewDirection::Out => Self::Out,
            _ => Self::Unsupported,
        }
    }
}
