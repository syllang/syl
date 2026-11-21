//! TIR side-table translation for an aggregated HIR arena.
//!
//! TIR tables are keyed by HIR ids and types may recursively contain definition
//! and local ids. This module owns that second-stage translation; HIR payload
//! copying remains in [`super::aggregate_remap`].

use std::collections::HashMap;
use std::sync::Arc;

use syl_hir::HirEnumVariantKey;

use super::{OwnerSlice, Remap};
use crate::hir::HirDesign;
use crate::tir::{BindingRef, RemappedTirTables, TirDesign, TirTypeTable, TypeId};

pub(super) fn remap_tir(
    hir: Arc<HirDesign>,
    slices: &[OwnerSlice<'_>],
    remaps: &[Remap],
) -> TirDesign {
    let mut type_table = TirTypeTable::new();
    let mut enum_values = std::collections::BTreeMap::new();
    let mut phases = std::collections::BTreeMap::new();
    let mut expr_types = std::collections::BTreeMap::new();
    let mut binding_kinds = std::collections::BTreeMap::new();
    let mut binding_types = std::collections::BTreeMap::new();
    for (index, slice) in slices.iter().enumerate() {
        let Some(tir) = slice.tir else {
            continue;
        };
        let remap = &remaps[index];
        let mut cache = HashMap::<TypeId, TypeId>::new();
        for (key, value) in tir.enum_variant_values() {
            if !remap.copied.contains(&key.enum_def) {
                continue;
            }
            let Some(new_def) = remap.defs.get(&key.enum_def).copied() else {
                continue;
            };
            enum_values.insert(HirEnumVariantKey::new(new_def, key.name.clone()), *value);
        }
        for (expr, phase) in tir.expr_phases() {
            if let Some(new_expr) = remap.exprs.get(expr).copied() {
                phases.insert(new_expr, *phase);
            }
        }
        for (expr, ty) in tir.expr_types() {
            if let Some(new_expr) = remap.exprs.get(expr).copied() {
                expr_types.insert(
                    new_expr,
                    intern_type(*ty, tir, remap, &mut cache, &mut type_table),
                );
            }
        }
        for (binding, kind) in tir.binding_kinds() {
            if let Some(mapped) = remap_binding(*binding, remap) {
                binding_kinds.insert(mapped, *kind);
            }
        }
        for (binding, ty) in tir.binding_types() {
            if let Some(mapped) = remap_binding(*binding, remap) {
                binding_types.insert(
                    mapped,
                    intern_type(*ty, tir, remap, &mut cache, &mut type_table),
                );
            }
        }
    }
    TirDesign::from_remapped(
        hir,
        RemappedTirTables {
            type_table,
            enum_variant_values: enum_values,
            expr_phases: phases,
            expr_types,
            binding_kinds,
            binding_types,
        },
    )
}

fn remap_binding(binding: BindingRef, remap: &Remap) -> Option<BindingRef> {
    match binding {
        BindingRef::Def(id) if remap.copied.contains(&id) => {
            remap.defs.get(&id).copied().map(BindingRef::Def)
        }
        BindingRef::Local(id) => remap.locals.get(&id).copied().map(BindingRef::Local),
        BindingRef::Def(_) => None,
    }
}

fn intern_type(
    old: TypeId,
    tir: &TirDesign,
    remap: &Remap,
    cache: &mut HashMap<TypeId, TypeId>,
    dest: &mut TirTypeTable,
) -> TypeId {
    if let Some(id) = cache.get(&old).copied() {
        return id;
    }
    let ty = tir
        .type_table()
        .get(old)
        .cloned()
        .unwrap_or(crate::tir::TirType::Unknown);
    let remapped = ty.remap_ids(&|id| remap.defs.get(&id).copied(), &|id| {
        remap.locals.get(&id).copied()
    });
    let id = dest.intern(remapped);
    cache.insert(old, id);
    id
}
