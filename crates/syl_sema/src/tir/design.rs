use super::binding::{BindingKind, BindingRef};
use super::phase::Phase;
use super::type_system::{TirType, TirTypeTable, TypeId};
#[cfg(test)]
use crate::hir::HirDefKind;
use crate::hir::HirDesign;
use derive_builder::Builder;
use getset::Getters;
use std::{collections::BTreeMap, sync::Arc};
#[cfg(test)]
use syl_hir::DefId;
use syl_hir::{ExprId, HirEnumVariantKey};

/// Typed analysis aggregate over a resolved [`HirDesign`].
///
/// Aggregate root for TIR: owns the HIR snapshot plus type/phase/binding side
/// tables keyed by stable HIR ids. Fields are private; use derived getters
/// (`type_table()`, `expr_types()`, …) and the custom `hir()` projection.
/// Prefer [`TypePhaseChecker`](super::TypePhaseChecker) for construction —
/// there are no public mutators because a finished TIR design is read-only.
///
/// Built by `TypePhaseChecker::finish` via [`TirDesignBuilder`], then consumed
/// by facts, elab, query, and IDE layers.
#[derive(Getters, Builder)]
#[getset(get = "pub")]
#[builder(pattern = "owned", build_fn(name = "try_build"), vis = "pub(super)")]
#[non_exhaustive]
pub struct TirDesign {
    /// Underlying HIR; exposed as `&HirDesign` via [`Self::hir`], not as `Arc`.
    #[getset(skip)]
    hir: Arc<HirDesign>,
    type_table: TirTypeTable,
    enum_variant_values: BTreeMap<HirEnumVariantKey, u64>,
    expr_phases: BTreeMap<ExprId, Phase>,
    expr_types: BTreeMap<ExprId, TypeId>,
    binding_kinds: BTreeMap<BindingRef, BindingKind>,
    binding_types: BTreeMap<BindingRef, TypeId>,
}

impl TirDesignBuilder {
    /// Builds a finished TIR design. All fields must be set.
    pub(super) fn build(self) -> TirDesign {
        self.try_build()
            .expect("TirDesignBuilder fields must be complete")
    }
}

impl TirDesign {
    /// Returns the underlying resolved HIR design.
    pub fn hir(&self) -> &HirDesign {
        &self.hir
    }

    pub fn debug_dump(&self) -> String {
        format!(
            "tir hir_defs={} hir_locals={} enum_values={} expr_phases={} expr_types={} bindings={} binding_types={}",
            self.hir.defs().len(),
            self.hir.locals().len(),
            self.enum_variant_values.len(),
            self.expr_phases.len(),
            self.expr_types.len(),
            self.binding_kinds.len(),
            self.binding_types.len(),
        )
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
