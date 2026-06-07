use super::{TirConstEnv, TirConstStructValue, TirConstValue};
use crate::{
    ir::const_mir::{
        ConstEvalEnv, ConstMirBuilder, ConstMirProgram, ConstStructFieldValue, ConstStructValue,
        ConstValue,
    },
    tir::{TirDesign, TypePhaseChecker},
};
use std::collections::BTreeMap;

impl TirConstEnv {
    pub(super) fn const_call_value(
        &self,
        expr: &crate::hir::HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<TirConstValue> {
        let crate::hir::HirExprNode::Call { .. } = expr.node else {
            return None;
        };
        let tir = TirDesign {
            hir: checker.hir.clone(),
            type_table: checker.type_table.clone(),
            enum_variant_values: checker.enum_variant_values.clone(),
            expr_phases: checker.expr_phases.clone(),
            expr_types: checker.expr_types.clone(),
            binding_kinds: checker.binding_kinds.clone(),
            binding_types: checker.binding_types.clone(),
        };
        let lowering = ConstMirBuilder::new(&tir);
        let lowered = lowering.lower_const_expr(self.owner, expr);
        let program = lowering.build().ok()?;
        let mut env = ConstEvalEnv::with_owner(Some(self.owner));
        for (id, binding) in &self.bindings {
            let Some(value) = binding
                .value
                .as_ref()
                .and_then(|value| value.to_const(&program))
            else {
                continue;
            };
            let name = checker
                .hir
                .locals
                .iter()
                .find(|local| local.id == *id)?
                .name
                .clone();
            env.bind(name, value);
        }
        let mut evaluator = program.evaluator();
        TirConstValue::from_const(evaluator.expr_value(&lowered, &mut env).ok()?)
    }
}

impl TirConstValue {
    fn from_const(value: ConstValue) -> Option<Self> {
        match value {
            ConstValue::Nat(value) => Some(Self::Nat(value)),
            ConstValue::Bool(value) => Some(Self::Bool(value)),
            ConstValue::Struct(value) => Some(Self::Struct(TirConstStructValue {
                def: value.kind().def(),
                fields: value
                    .fields()
                    .iter()
                    .map(|field| {
                        Some((
                            field.name().to_string(),
                            Self::from_const(field.value().clone())?,
                        ))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()?,
            })),
            ConstValue::Unknown(_) => None,
        }
    }

    fn to_const(&self, program: &ConstMirProgram) -> Option<ConstValue> {
        match self {
            Self::Nat(value) => Some(ConstValue::Nat(*value)),
            Self::Bool(value) => Some(ConstValue::Bool(*value)),
            Self::Struct(value) => Some(ConstValue::Struct(ConstStructValue::new(
                program.struct_kind(value.def)?,
                value
                    .fields
                    .iter()
                    .map(|(name, value)| {
                        Some(ConstStructFieldValue::new(
                            name.clone(),
                            value.to_const(program)?,
                        ))
                    })
                    .collect::<Option<Vec<_>>>()?,
            ))),
        }
    }
}
