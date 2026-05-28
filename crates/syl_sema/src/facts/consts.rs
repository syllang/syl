use super::HirFactId;
use crate::{
    ir::const_mir::{ConstEvalEnv, ConstMirBuilder, ConstValue},
    tir::TirDesign,
};
use std::collections::BTreeMap;
use syl_hir::DefId;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ConstFactKey {
    Def(DefId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ConstFacts {
    values: BTreeMap<HirFactId, ConstValue>,
    cache: BTreeMap<ConstFactKey, ConstValue>,
}

impl ConstFacts {
    pub(crate) fn collect(tir: &TirDesign) -> Self {
        let lowering = ConstMirBuilder::new(tir);
        let Ok(program) = lowering.build() else {
            return Self {
                values: BTreeMap::new(),
                cache: BTreeMap::new(),
            };
        };
        let mut evaluator = program.evaluator();
        let mut values = BTreeMap::new();
        let mut cache = BTreeMap::new();

        for def in tir.hir().consts.keys().copied() {
            let Some(item) = tir.hir().consts.get(&def) else {
                continue;
            };
            let expr = lowering.lower_const_expr(def, &item.value);
            let mut env = ConstEvalEnv::with_owner(Some(def));
            let Ok(value) = evaluator.expr_value(&expr, &mut env) else {
                continue;
            };
            values.insert(HirFactId::Def(def), value);
            cache.insert(ConstFactKey::Def(def), value);
            for (expr_id, expr_value) in evaluator.recorded_expr_values() {
                values.insert(HirFactId::Expr(*expr_id), *expr_value);
            }
        }

        Self { values, cache }
    }

    pub fn value(&self, id: HirFactId) -> Option<ConstValue> {
        self.values.get(&id).copied()
    }

    pub fn cache_value(&self, key: ConstFactKey) -> Option<ConstValue> {
        self.cache.get(&key).copied()
    }
}
