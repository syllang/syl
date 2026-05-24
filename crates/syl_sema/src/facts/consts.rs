use super::HirFactId;
use crate::{
    const_eval::{ConstKind, ConstValue},
    hir::{HirDesign, HirExprNode},
    mir::MirTypeRef,
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hir::{DefId, ExprId, HirResolution};
use syl_syntax::{BinaryOp, UnaryOp};

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
    pub(crate) fn empty() -> Self {
        Self {
            values: BTreeMap::new(),
            cache: BTreeMap::new(),
        }
    }

    pub(crate) fn collect(hir: &HirDesign) -> Self {
        let mut builder = ConstFactBuilder::new(hir);
        let defs: Vec<_> = hir.consts.keys().copied().collect();
        for def in defs {
            let _ = builder.const_value_for_def(def);
        }
        Self {
            values: builder.values,
            cache: builder.cache,
        }
    }

    pub fn value(&self, id: HirFactId) -> Option<ConstValue> {
        self.values.get(&id).copied()
    }

    pub fn cache_value(&self, key: ConstFactKey) -> Option<ConstValue> {
        self.cache.get(&key).copied()
    }
}

struct ConstFactBuilder<'a> {
    hir: &'a HirDesign,
    values: BTreeMap<HirFactId, ConstValue>,
    cache: BTreeMap<ConstFactKey, ConstValue>,
    visiting: BTreeSet<DefId>,
}

impl<'a> ConstFactBuilder<'a> {
    fn new(hir: &'a HirDesign) -> Self {
        Self {
            hir,
            values: BTreeMap::new(),
            cache: BTreeMap::new(),
            visiting: BTreeSet::new(),
        }
    }

    fn const_value_for_def(&mut self, def: DefId) -> Option<ConstValue> {
        if let Some(value) = self.cache.get(&ConstFactKey::Def(def)).copied() {
            return Some(value);
        }
        if !self.visiting.insert(def) {
            return None;
        }
        let Some(item) = self.hir.consts.get(&def) else {
            self.visiting.remove(&def);
            return None;
        };
        let value = self
            .const_value_for_expr(item.value.id(), &item.value)
            .or_else(|| {
                item.ty
                    .as_ref()
                    .and_then(const_kind_for_mir_type)
                    .map(ConstValue::Unknown)
            });
        self.visiting.remove(&def);
        if let Some(value) = value {
            self.values.insert(HirFactId::Def(def), value);
            self.cache.insert(ConstFactKey::Def(def), value);
        }
        value
    }

    fn const_value_for_expr(
        &mut self,
        expr_id: ExprId,
        expr: &crate::hir::HirBodyExpr,
    ) -> Option<ConstValue> {
        match &expr.node {
            HirExprNode::Int(value) => Some(ConstValue::Nat(*value)),
            HirExprNode::Bool(value) => Some(ConstValue::Bool(*value)),
            HirExprNode::Ident(_) => match self.hir.expr_resolutions.get(&expr_id).copied()? {
                HirResolution::Def(def) => self.const_value_for_def(def),
                HirResolution::Local(_) => None,
                _ => None,
            },
            HirExprNode::Group(inner) => self.const_value_for_expr(inner.id(), inner),
            HirExprNode::Unary {
                op: UnaryOp::Not,
                expr: inner,
            } => match self.const_value_for_expr(inner.id(), inner)? {
                ConstValue::Bool(value) => Some(ConstValue::Bool(!value)),
                ConstValue::Unknown(ConstKind::Bool) => Some(ConstValue::Unknown(ConstKind::Bool)),
                _ => None,
            },
            HirExprNode::Binary { op, left, right } => {
                let lhs = self.const_value_for_expr(left.id(), left)?;
                let rhs = self.const_value_for_expr(right.id(), right)?;
                const_binary_result(*op, lhs, rhs)
            }
            _ => None,
        }
    }
}

fn const_binary_result(op: BinaryOp, lhs: ConstValue, rhs: ConstValue) -> Option<ConstValue> {
    match (op, lhs, rhs) {
        (BinaryOp::EqEq, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Bool(left == right))
        }
        (BinaryOp::EqEq, ConstValue::Bool(left), ConstValue::Bool(right)) => {
            Some(ConstValue::Bool(left == right))
        }
        (BinaryOp::NotEq, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Bool(left != right))
        }
        (BinaryOp::NotEq, ConstValue::Bool(left), ConstValue::Bool(right)) => {
            Some(ConstValue::Bool(left != right))
        }
        (BinaryOp::Lt, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Bool(left < right))
        }
        (BinaryOp::LtEq, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Bool(left <= right))
        }
        (BinaryOp::Gt, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Bool(left > right))
        }
        (BinaryOp::GtEq, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Bool(left >= right))
        }
        (BinaryOp::AndAnd, ConstValue::Bool(left), ConstValue::Bool(right)) => {
            Some(ConstValue::Bool(left && right))
        }
        (BinaryOp::OrOr, ConstValue::Bool(left), ConstValue::Bool(right)) => {
            Some(ConstValue::Bool(left || right))
        }
        (BinaryOp::Add, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Nat(left + right))
        }
        (BinaryOp::Sub, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Nat(left.saturating_sub(right)))
        }
        (BinaryOp::Mul, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Nat(left * right))
        }
        (BinaryOp::Div, ConstValue::Nat(left), ConstValue::Nat(right)) if right != 0 => {
            Some(ConstValue::Nat(left / right))
        }
        (BinaryOp::Rem, ConstValue::Nat(left), ConstValue::Nat(right)) if right != 0 => {
            Some(ConstValue::Nat(left % right))
        }
        (BinaryOp::Shl, ConstValue::Nat(left), ConstValue::Nat(right)) => {
            Some(ConstValue::Nat(left << right))
        }
        (BinaryOp::EqEq | BinaryOp::NotEq, ConstValue::Unknown(_), ConstValue::Unknown(_)) => {
            Some(ConstValue::Unknown(ConstKind::Bool))
        }
        _ => None,
    }
}

fn const_kind_for_mir_type(ty: &MirTypeRef) -> Option<ConstKind> {
    let mut current = ty;
    loop {
        if let Some(name) = current.path_name() {
            return match name {
                "Nat" => Some(ConstKind::Nat),
                "Bool" => Some(ConstKind::Bool),
                _ => None,
            };
        }
        if let Some(base) = current.generic_base() {
            current = base;
            continue;
        }
        if let Some((base, _)) = current.view_select() {
            current = base;
            continue;
        }
        if let Some((_, elem)) = current.array() {
            current = elem;
            continue;
        }
        return None;
    }
}
