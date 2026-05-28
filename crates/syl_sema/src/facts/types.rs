use super::HirFactId;
use crate::{
    TypeId,
    tir::{BindingRef, TirDesign, TirType, TirTypeTable},
};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TypeTable {
    types: TirTypeTable,
    values: BTreeMap<HirFactId, TypeId>,
}

impl TypeTable {
    pub(crate) fn collect(tir: &TirDesign) -> Self {
        let mut values = BTreeMap::new();
        for (expr, ty) in tir.expr_types() {
            values.insert(HirFactId::Expr(*expr), *ty);
        }
        for (binding, ty) in tir.binding_types() {
            let id = match binding {
                BindingRef::Def(def) => HirFactId::Def(*def),
                BindingRef::Local(local) => HirFactId::Local(*local),
            };
            values.insert(id, *ty);
        }
        Self {
            types: tir.type_table().clone(),
            values,
        }
    }

    pub fn get(&self, id: HirFactId) -> Option<TypeId> {
        self.values.get(&id).copied()
    }

    pub fn ty(&self, id: HirFactId) -> Option<&TirType> {
        self.get(id).and_then(|ty| self.types.get(ty))
    }

    pub fn type_table(&self) -> &TirTypeTable {
        &self.types
    }

    pub(crate) fn raw_values(&self) -> &BTreeMap<HirFactId, TypeId> {
        &self.values
    }
}
