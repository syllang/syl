use super::{FnTypeEnv, Phase, TirType, TypePhaseChecker};
use crate::hir::view::HirDesignViewExt;
use crate::{
    CompileError, TirError,
    hir::{HirBodyExpr, HirExprNode, resolve::HirResolution},
};
use syl_syntax::{BinaryOp, UnaryOp};

impl TypePhaseChecker {
    pub(super) fn check_fn_expr(
        &mut self,
        expr: &HirBodyExpr,
        env: &FnTypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<TirType, CompileError> {
        let owner = self.current_owner()?;
        self.checked_exprs += 1;
        let ty = match &expr.node {
            HirExprNode::Int(_) => TirType::Nat,
            HirExprNode::Bool(_) => TirType::Bool,
            HirExprNode::Str(_) => TirType::Str,
            HirExprNode::Ident(_) => match self.hir.expr_resolution(owner, expr).ok().flatten() {
                Some(HirResolution::Local(id)) => env
                    .locals
                    .get(&id)
                    .map(|local| local.ty.clone())
                    .unwrap_or_else(|| self.infer_expr_type(owner, expr)),
                _ => self.infer_expr_type(owner, expr),
            },
            HirExprNode::Group(inner) | HirExprNode::GenericApp { callee: inner, .. } => {
                self.check_fn_expr(inner, env, errors)?
            }
            HirExprNode::Unary { op, expr: inner } => {
                let actual = self.check_fn_expr(inner, env, errors)?;
                let expected = match op {
                    UnaryOp::Not => TirType::Bool,
                    UnaryOp::Neg => TirType::Nat,
                    _ => {
                        errors.push(CompileError::lowering_at(
                            TirError::InvalidElaborationExpression,
                            expr.span(),
                        ));
                        TirType::Unknown
                    }
                };
                self.check_fn_type(&expected, &actual, inner, errors);
                expected
            }
            HirExprNode::Binary { op, left, right } => {
                let lhs = self.check_fn_expr(left, env, errors)?;
                let rhs = self.check_fn_expr(right, env, errors)?;
                match op {
                    BinaryOp::Add
                    | BinaryOp::Sub
                    | BinaryOp::Mul
                    | BinaryOp::Div
                    | BinaryOp::Rem
                    | BinaryOp::Shl
                    | BinaryOp::Lt
                    | BinaryOp::LtEq
                    | BinaryOp::Gt
                    | BinaryOp::GtEq => {
                        self.check_fn_type(&TirType::Nat, &lhs, left, errors);
                        self.check_fn_type(&TirType::Nat, &rhs, right, errors);
                        if matches!(
                            op,
                            BinaryOp::Lt | BinaryOp::LtEq | BinaryOp::Gt | BinaryOp::GtEq
                        ) {
                            TirType::Bool
                        } else {
                            TirType::Nat
                        }
                    }
                    BinaryOp::AndAnd | BinaryOp::OrOr => {
                        self.check_fn_type(&TirType::Bool, &lhs, left, errors);
                        self.check_fn_type(&TirType::Bool, &rhs, right, errors);
                        TirType::Bool
                    }
                    BinaryOp::EqEq | BinaryOp::NotEq => {
                        self.check_fn_type(&lhs, &rhs, right, errors);
                        TirType::Bool
                    }
                    _ => {
                        errors.push(CompileError::lowering_at(
                            TirError::InvalidElaborationExpression,
                            expr.span(),
                        ));
                        TirType::Unknown
                    }
                }
            }
            HirExprNode::Call { callee, args } => self.check_fn_call(callee, args, env, errors)?,
            HirExprNode::Aggregate { ty, fields } => {
                let aggregate =
                    Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, ty))
                        .unwrap_or(TirType::Unknown);
                let declarations = aggregate
                    .definition()
                    .and_then(|def| {
                        self.hir
                            .structs()
                            .get(&def)
                            .map(|item| &item.fields)
                            .or_else(|| self.hir.bundles().get(&def).map(|item| &item.fields))
                    })
                    .cloned();
                if let Some(declarations) = declarations {
                    let mut seen = std::collections::BTreeSet::new();
                    for field in fields {
                        let actual = self.check_fn_expr(&field.value, env, errors)?;
                        if !seen.insert(&field.name) {
                            errors.push(CompileError::lowering_at(
                                crate::EirError::DuplicateArgument {
                                    name: field.name.clone(),
                                },
                                field.value.span(),
                            ));
                        }
                        if let Some(expected) = self.field_type_for(&aggregate, &field.name) {
                            self.check_fn_type(&expected, &actual, &field.value, errors);
                        } else {
                            errors.push(CompileError::lowering_at(
                                TirError::UnknownAggregateField {
                                    ty: aggregate.label(),
                                    field: field.name.clone(),
                                },
                                field.value.span(),
                            ));
                        }
                    }
                    for field in declarations {
                        if !fields.iter().any(|value| value.name == field.name) {
                            errors.push(CompileError::lowering_at(
                                TirError::MissingAggregateField {
                                    ty: aggregate.label(),
                                    field: field.name,
                                },
                                expr.span(),
                            ));
                        }
                    }
                } else {
                    for field in fields {
                        self.check_fn_expr(&field.value, env, errors)?;
                    }
                    if !matches!(aggregate, TirType::Unknown) {
                        errors.push(CompileError::lowering_at(
                            TirError::InvalidElaborationExpression,
                            expr.span(),
                        ));
                    }
                }
                aggregate
            }
            HirExprNode::Field { base, field } => {
                let base_ty = self.check_fn_expr(base, env, errors)?;
                if self.hir.enum_variant_expr(expr).is_some() {
                    self.infer_expr_type(owner, expr)
                } else if let Some(ty) = self.field_type_for(&base_ty, field) {
                    ty
                } else {
                    if !matches!(base_ty, TirType::Unknown) {
                        errors.push(CompileError::lowering_at(
                            TirError::UnknownAggregateField {
                                ty: base_ty.label(),
                                field: field.clone(),
                            },
                            expr.span(),
                        ));
                    }
                    TirType::Unknown
                }
            }
            HirExprNode::Block(block) => {
                self.check_fn_block(block, &mut env.clone(), errors)?;
                block
                    .tail
                    .as_ref()
                    .map(|tail| self.fn_expr_type(tail))
                    .unwrap_or(TirType::Unknown)
            }
            HirExprNode::Match { expr: target, arms } => {
                let target_ty = self.check_fn_expr(target, env, errors)?;
                if arms.is_empty() {
                    errors.push(CompileError::lowering_at(
                        TirError::MatchRequiresArm,
                        expr.span(),
                    ));
                }
                for arm in arms {
                    use crate::ir::mir::MirPattern;
                    let pattern_ty = match &arm.pattern {
                        MirPattern::Bool(_, _) => TirType::Bool,
                        MirPattern::Int(_, _) => TirType::Nat,
                        _ => TirType::Unknown,
                    };
                    if !self.types_compatible_for_return(
                        &target_ty,
                        &pattern_ty,
                        matches!(arm.pattern, MirPattern::Int(_, _)),
                    ) {
                        errors.push(CompileError::lowering_at(
                            TirError::TypeMismatch {
                                expected: target_ty.label(),
                                actual: pattern_ty.label(),
                            },
                            expr.span(),
                        ));
                    }
                    self.check_fn_expr(&arm.value, env, errors)?;
                }
                self.check_branch_types(arms.iter().map(|arm| &arm.value), errors);
                self.infer_branch_type(owner, arms.iter().map(|arm| &arm.value))
            }
            HirExprNode::Place { callee, args, .. } => {
                self.check_fn_expr(callee, env, errors)?;
                for arg in args {
                    self.check_fn_expr(&arg.value, env, errors)?;
                }
                errors.push(CompileError::lowering_at(
                    TirError::HardwareStatementInFn,
                    expr.span(),
                ));
                TirType::Unknown
            }
            HirExprNode::Select { arms, .. } => {
                for arm in arms {
                    if !matches!(&arm.pattern.node, HirExprNode::Ident(name) if name == "default") {
                        self.check_fn_expr(&arm.pattern, env, errors)?;
                    }
                    self.check_fn_expr(&arm.value, env, errors)?;
                }
                errors.push(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    expr.span(),
                ));
                TirType::Unknown
            }
            _ => {
                errors.push(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    expr.span(),
                ));
                TirType::Unknown
            }
        };
        self.record_expr_type(expr, ty.clone())?;
        self.record_phase(expr, Phase::Const)?;
        Ok(ty)
    }

    pub(super) fn fn_expr_type(&self, expr: &HirBodyExpr) -> TirType {
        self.expr_types
            .get(&expr.id())
            .and_then(|id| self.type_table.get(*id))
            .cloned()
            .unwrap_or(TirType::Unknown)
    }
}
