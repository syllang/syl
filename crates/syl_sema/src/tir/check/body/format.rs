use super::*;
use syl_hir::{FormatPart, HirDefKind};

fn invalid(message: impl Into<String>, span: Span) -> CompileError {
    CompileError::lowering_at(
        EirError::InvalidFormat {
            message: message.into(),
        },
        span,
    )
}

impl TypePhaseChecker {
    pub(super) fn check_clocked_message(
        &mut self,
        expr: &HirBodyExpr,
        kind: BuiltinIntrinsic,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let HirExprNode::Call { args, .. } = &expr.node else {
            return Ok(());
        };
        let owner = self.current_owner()?;
        let mut names = std::collections::BTreeSet::new();
        let mut positional = Vec::new();
        for arg in args {
            let Some(name) = arg.name.as_deref() else {
                positional.push(arg);
                continue;
            };
            if !names.insert(name) {
                errors.push(invalid(format!("duplicate argument '{name}'"), arg.span()));
            }
            self.check_hardware_value_expr(&arg.value, errors)?;
            let ty = self.infer_expr_type(owner, &arg.value);
            let valid = match name {
                "clk" => matches!(ty, TirType::Clock { .. }),
                "rst" => matches!(ty, TirType::Reset { .. }),
                "when" if kind == BuiltinIntrinsic::Log => ty == TirType::Bit,
                _ => {
                    errors.push(invalid(format!("unknown argument '{name}'"), arg.span()));
                    continue;
                }
            };
            if !valid {
                errors.push(invalid(
                    format!(
                        "'{name}' requires a {} expression",
                        match name {
                            "clk" => "Clock",
                            "rst" => "Reset",
                            _ => "Bit",
                        }
                    ),
                    arg.value.span(),
                ));
            }
        }
        let mut positional = positional.as_slice();
        if kind == BuiltinIntrinsic::Assert {
            let Some((cond, rest)) = positional.split_first() else {
                return Err(CompileError::lowering_at(
                    EirError::AssertionRequiresSingleCondition,
                    expr.span(),
                ));
            };
            self.check_hardware_value_expr(&cond.value, errors)?;
            if self.infer_expr_type(owner, &cond.value) != TirType::Bit {
                errors.push(CompileError::lowering_at(
                    EirError::AssertionConditionMustBeBit,
                    cond.value.span(),
                ));
            }
            positional = rest;
            if positional.is_empty() {
                return Ok(());
            }
        }
        let Some((message, values)) = positional.split_first() else {
            return Err(invalid("expected a format string literal", expr.span()));
        };
        let Some(template) = &message.format else {
            return Err(invalid("expected a format string literal", message.span()));
        };
        let parts = template.as_ref().map_err(|e| invalid(&e.message, e.span))?;
        let mut used = vec![false; values.len()];
        for part in parts {
            let value = match part {
                FormatPart::Literal(_) => continue,
                FormatPart::Capture { expr, .. } => expr,
                FormatPart::Argument { index, .. } => {
                    let Some(arg) = values.get(*index) else {
                        errors.push(invalid(
                            format!("format argument {index} is missing"),
                            message.span(),
                        ));
                        continue;
                    };
                    used[*index] = true;
                    &arg.value
                }
            };
            let ty = self.infer_expr_type(owner, value);
            if matches!(
                ty,
                TirType::Bool
                    | TirType::Named {
                        kind: Some(HirDefKind::Struct),
                        ..
                    }
            ) {
                self.check_format_const_expr(value, errors)?;
            } else {
                self.check_hardware_value_expr(value, errors)?;
            }
            if !matches!(
                ty,
                TirType::Nat
                    | TirType::Bool
                    | TirType::Bit
                    | TirType::UInt { .. }
                    | TirType::Bits { .. }
                    | TirType::SInt { .. }
                    | TirType::Named {
                        kind: Some(HirDefKind::Enum | HirDefKind::Bundle | HirDefKind::Struct),
                        ..
                    }
            ) {
                errors.push(invalid(
                    format!("cannot format value of type {ty:?}"),
                    value.span(),
                ));
            }
        }
        for (arg, used) in values.iter().zip(used) {
            if !used {
                errors.push(invalid("unused positional format argument", arg.span()));
            }
        }
        Ok(())
    }
}
