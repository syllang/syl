use super::binding::{BindingKind, BindingRef};
use super::phase::Phase;
use super::type_system::{TirType, TirTypeTable, TypeId};
#[cfg(test)]
use crate::hir::HirDefKind;
use crate::hir::HirDesign;
use std::{collections::BTreeMap, sync::Arc};
#[cfg(test)]
use syl_hir::DefId;
use syl_hir::{ExprId, HirEnumVariantKey};

#[non_exhaustive]
pub struct TirDesign {
    pub(super) hir: Arc<HirDesign>,
    pub(super) type_table: TirTypeTable,
    pub(super) enum_variant_values: BTreeMap<HirEnumVariantKey, u64>,
    pub(super) expr_phases: BTreeMap<ExprId, Phase>,
    pub(super) expr_types: BTreeMap<ExprId, TypeId>,
    pub(super) binding_kinds: BTreeMap<BindingRef, BindingKind>,
    pub(super) binding_types: BTreeMap<BindingRef, TypeId>,
}

impl TirDesign {
    pub fn hir(&self) -> &HirDesign {
        &self.hir
    }

    pub fn debug_dump(&self) -> String {
        format!(
            "tir hir_defs={} hir_locals={} enum_values={} expr_phases={} expr_types={} bindings={} binding_types={}",
            self.hir.defs.len(),
            self.hir.locals.len(),
            self.enum_variant_values.len(),
            self.expr_phases.len(),
            self.expr_types.len(),
            self.binding_kinds.len(),
            self.binding_types.len(),
        )
    }

    pub fn expr_phases(&self) -> &BTreeMap<ExprId, Phase> {
        &self.expr_phases
    }

    pub fn binding_kinds(&self) -> &BTreeMap<BindingRef, BindingKind> {
        &self.binding_kinds
    }

    pub fn type_table(&self) -> &TirTypeTable {
        &self.type_table
    }

    pub fn enum_variant_values(&self) -> &BTreeMap<HirEnumVariantKey, u64> {
        &self.enum_variant_values
    }

    pub fn expr_types(&self) -> &BTreeMap<ExprId, TypeId> {
        &self.expr_types
    }

    pub fn binding_types(&self) -> &BTreeMap<BindingRef, TypeId> {
        &self.binding_types
    }

    pub fn type_count(&self) -> usize {
        self.expr_types.len() + self.binding_types.len()
    }

    #[cfg(test)]
    pub fn binding_type_definition(&self, binding: BindingRef) -> Option<DefId> {
        self.binding_types
            .get(&binding)
            .and_then(|ty| self.type_table.get(*ty))
            .and_then(TirType::definition)
    }

    #[cfg(test)]
    pub fn binding_type_definition_kind(&self, binding: BindingRef) -> Option<HirDefKind> {
        self.binding_types
            .get(&binding)
            .and_then(|ty| self.type_table.get(*ty))
            .and_then(TirType::definition_kind)
    }

    #[cfg(test)]
    pub fn binding_type_label(&self, binding: BindingRef) -> Option<String> {
        self.binding_types
            .get(&binding)
            .and_then(|ty| self.type_table.get(*ty))
            .map(TirType::label)
    }

    pub fn known_type_label(&self, id: ExprId) -> Option<String> {
        self.expr_types
            .get(&id)
            .and_then(|ty| self.type_table.get(*ty))
            .and_then(|ty| (!matches!(ty, TirType::Unknown)).then(|| ty.label()))
    }
}
