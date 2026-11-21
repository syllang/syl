use super::{TirConstEnv, TirConstValue};
use crate::{
    hir::{HirBodyExpr, HirExprNode},
    tir::TypePhaseChecker,
};
use syl_syntax::BinaryOp;

impl TirConstEnv {
    pub(in crate::tir) fn const_bool_value(
        &self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<bool> {
        match &expr.node {
            HirExprNode::Bool(value) => Some(*value),
            HirExprNode::Ident(_) => self
                .local_binding(expr, checker)
                .and_then(|binding| match &binding.value {
                    Some(TirConstValue::Bool(value)) => Some(*value),
                    _ => None,
                })
                .or_else(|| {
                    self.follow_const(expr, checker, |env, item| {
                        env.const_bool_value(&item.value, checker)
                    })
                }),
            HirExprNode::Call { .. } => self.const_call_value(expr, checker).and_then(|value| {
                if let TirConstValue::Bool(value) = value {
                    Some(value)
                } else {
                    None
                }
            }),
            HirExprNode::Group(expr) => self.const_bool_value(expr, checker),
            HirExprNode::Field { base, field } => self
                .const_struct_value(base, checker)
                .and_then(|value| value.field_bool(field))
                .or_else(|| {
                    self.field_value_expr(base, field, checker)
                        .and_then(|value| self.const_bool_value(value, checker))
                }),
            HirExprNode::Unary {
                op: syl_syntax::UnaryOp::Not,
                expr,
            } => self.const_bool_value(expr, checker).map(|value| !value),
            HirExprNode::Binary {
                op, left, right, ..
            } => self.const_binary_bool_value(*op, left, right, checker),
            _ => None,
        }
    }

    fn const_binary_bool_value(
        &self,
        op: BinaryOp,
        left: &HirBodyExpr,
        right: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<bool> {
        match op {
            BinaryOp::EqEq => {
                if let (Some(lhs), Some(rhs)) = (
                    self.const_nat_value(left, checker),
                    self.const_nat_value(right, checker),
                ) {
                    return Some(lhs == rhs);
                }
                if let (Some(lhs), Some(rhs)) = (
                    self.const_bool_value(left, checker),
                    self.const_bool_value(right, checker),
                ) {
                    return Some(lhs == rhs);
                }
                None
            }
            BinaryOp::NotEq => {
                if let (Some(lhs), Some(rhs)) = (
                    self.const_nat_value(left, checker),
                    self.const_nat_value(right, checker),
                ) {
                    return Some(lhs != rhs);
                }
                if let (Some(lhs), Some(rhs)) = (
                    self.const_bool_value(left, checker),
                    self.const_bool_value(right, checker),
                ) {
                    return Some(lhs != rhs);
                }
                None
            }
            BinaryOp::Lt => {
                Some(self.const_nat_value(left, checker)? < self.const_nat_value(right, checker)?)
            }
            BinaryOp::LtEq => {
                Some(self.const_nat_value(left, checker)? <= self.const_nat_value(right, checker)?)
            }
            BinaryOp::Gt => {
                Some(self.const_nat_value(left, checker)? > self.const_nat_value(right, checker)?)
            }
            BinaryOp::GtEq => {
                Some(self.const_nat_value(left, checker)? >= self.const_nat_value(right, checker)?)
            }
            BinaryOp::AndAnd => Some(
                self.const_bool_value(left, checker)? && self.const_bool_value(right, checker)?,
            ),
            BinaryOp::OrOr => Some(
                self.const_bool_value(left, checker)? || self.const_bool_value(right, checker)?,
            ),
            _ => None,
        }
    }

    pub(in crate::tir) fn const_range_bounds(
        &self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<(u64, u64)> {
        let HirExprNode::Range { start, end } = &expr.node else {
            return None;
        };
        Some((
            self.const_nat_value(start, checker)?,
            self.const_nat_value(end, checker)?,
        ))
    }

    pub(in crate::tir) fn const_nat_value(
        &self,
        expr: &HirBodyExpr,
        checker: &TypePhaseChecker,
    ) -> Option<u64> {
        match &expr.node {
            HirExprNode::Int(value) => Some(*value),
            HirExprNode::Ident(_) => self
                .local_binding(expr, checker)
                .and_then(|binding| match &binding.value {
                    Some(TirConstValue::Nat(value)) => Some(*value),
                    _ => None,
                })
                .or_else(|| {
                    self.follow_const(expr, checker, |env, item| {
                        env.const_nat_value(&item.value, checker)
                    })
                }),
            HirExprNode::Call { .. } => self.const_call_value(expr, checker).and_then(|value| {
                if let TirConstValue::Nat(value) = value {
                    Some(value)
                } else {
                    None
                }
            }),
            HirExprNode::Group(expr) => self.const_nat_value(expr, checker),
            HirExprNode::Field { base, field } => self
                .const_struct_value(base, checker)
                .and_then(|value| value.field_nat(field))
                .or_else(|| {
                    self.field_value_expr(base, field, checker)
                        .and_then(|value| self.const_nat_value(value, checker))
                }),
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                let lhs = self.const_nat_value(left, checker)?;
                let rhs = self.const_nat_value(right, checker)?;
                match op {
                    BinaryOp::Add => lhs.checked_add(rhs),
                    BinaryOp::Sub => Some(lhs.saturating_sub(rhs)),
                    BinaryOp::Mul => lhs.checked_mul(rhs),
                    BinaryOp::Div if rhs != 0 => Some(lhs / rhs),
                    BinaryOp::Rem if rhs != 0 => Some(lhs % rhs),
                    BinaryOp::Shl => lhs.checked_shl(rhs.try_into().ok()?),
                    _ => None,
                }
            }
            _ => None,
        }
    }
}
