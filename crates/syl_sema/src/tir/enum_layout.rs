use super::TypePhaseChecker;
use super::{TirConstTerm, TirType};
use crate::ir::mir::MirTypeRef;
use crate::{
    CompileError, TirError,
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{HirBodyExpr, HirConstItem, HirEnumItem, HirEnumLayout, HirEnumVariantDecl, HirExprNode},
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hir::{DefId, HirEnumVariantKey};
use syl_syntax::BinaryOp;

pub(super) fn resolve_enum_values(
    checker: &TypePhaseChecker,
    owner: DefId,
    item: &HirEnumItem,
) -> Result<BTreeMap<HirEnumVariantKey, u64>, CompileError> {
    let width = item
        .width
        .as_ref()
        .map(|width| enum_width_value(checker, owner, width, &item.name))
        .transpose()?;
    let mut resolver = EnumValueResolver::new(checker, owner, item, width);
    resolver.resolve()
}

struct EnumValueResolver<'a> {
    checker: &'a TypePhaseChecker,
    owner: DefId,
    item: &'a HirEnumItem,
    width: Option<u64>,
    values_by_name: BTreeMap<String, u64>,
    values: BTreeMap<HirEnumVariantKey, u64>,
    seen_values: BTreeSet<u64>,
    const_stack: BTreeSet<DefId>,
}

/// Arguments for a binary expression within enum value evaluation.
struct BinaryExpr<'a> {
    op: BinaryOp,
    left: &'a HirBodyExpr,
    right: &'a HirBodyExpr,
}

impl<'a> EnumValueResolver<'a> {
    fn new(
        checker: &'a TypePhaseChecker,
        owner: DefId,
        item: &'a HirEnumItem,
        width: Option<u64>,
    ) -> Self {
        Self {
            checker,
            owner,
            item,
            width,
            values_by_name: BTreeMap::new(),
            values: BTreeMap::new(),
            seen_values: BTreeSet::new(),
            const_stack: BTreeSet::new(),
        }
    }

    fn resolve(&mut self) -> Result<BTreeMap<HirEnumVariantKey, u64>, CompileError> {
        let mut last_ordinal: Option<u64> = None;
        let layout: &'static str = self.item.layout.into();
        for (index, variant) in self.item.variants.iter().enumerate() {
            let value = match layout {
                "ordinal" => {
                    if let Some(expr) = &variant.value {
                        self.eval_nat_expr(expr, &variant.name)?
                    } else {
                        match last_ordinal {
                            Some(previous) => previous
                                .checked_add(1)
                                .ok_or_else(|| self.nat_error(variant.span, &variant.name))?,
                            None => 0,
                        }
                    }
                }
                "flags" => {
                    if let Some(expr) = &variant.value {
                        self.eval_nat_expr(expr, &variant.name)?
                    } else {
                        one_hot_value(index)
                            .ok_or_else(|| self.nat_error(variant.span, &variant.name))?
                    }
                }
                "onehot" => {
                    if let Some(expr) = &variant.value {
                        self.eval_nat_expr(expr, &variant.name)?
                    } else {
                        one_hot_value(index)
                            .ok_or_else(|| self.nat_error(variant.span, &variant.name))?
                    }
                }
                _ => {
                    return Err(CompileError::lowering_at(
                        TirError::RequiresNatExpression {
                            context: format!("enum layout for {}", self.item.name),
                        },
                        variant.span,
                    ));
                }
            };
            self.validate_layout_value(variant, value)?;
            if !self.seen_values.insert(value) {
                return Err(CompileError::lowering_at(
                    TirError::DuplicateEnumDiscriminant {
                        enum_name: self.item.name.clone(),
                        value,
                    },
                    variant.span,
                ));
            }
            self.values_by_name.insert(variant.name.clone(), value);
            self.values.insert(
                HirEnumVariantKey::new(self.owner, variant.name.clone()),
                value,
            );
            if matches!(self.item.layout, HirEnumLayout::Ordinal) {
                last_ordinal = Some(value);
            }
            if let Some(width) = self.width
                && !fits_width(width, value)
            {
                return Err(CompileError::lowering_at(
                    TirError::EnumDiscriminantOutOfRange {
                        enum_name: self.item.name.clone(),
                        variant: variant.name.clone(),
                        value,
                        width,
                    },
                    variant.span,
                ));
            }
        }
        Ok(std::mem::take(&mut self.values))
    }

    fn validate_layout_value(
        &self,
        variant: &HirEnumVariantDecl,
        value: u64,
    ) -> Result<(), CompileError> {
        let layout: &'static str = self.item.layout.into();
        match layout {
            "ordinal" => Ok(()),
            "flags" => {
                if value == 0 || value.is_power_of_two() {
                    Ok(())
                } else {
                    Err(CompileError::lowering_at(
                        TirError::EnumDiscriminantNotOneHot {
                            enum_name: self.item.name.clone(),
                            variant: variant.name.clone(),
                            value,
                        },
                        variant.span,
                    ))
                }
            }
            "onehot" => {
                if value.is_power_of_two() {
                    Ok(())
                } else {
                    Err(CompileError::lowering_at(
                        TirError::EnumDiscriminantNotOneHot {
                            enum_name: self.item.name.clone(),
                            variant: variant.name.clone(),
                            value,
                        },
                        variant.span,
                    ))
                }
            }
            _ => Err(CompileError::lowering_at(
                TirError::RequiresNatExpression {
                    context: format!("enum layout for {}", self.item.name),
                },
                variant.span,
            )),
        }
    }

    fn eval_nat_expr(
        &mut self,
        expr: &HirBodyExpr,
        variant_name: &str,
    ) -> Result<u64, CompileError> {
        match &expr.node {
            HirExprNode::Int(value) => Ok(*value),
            HirExprNode::Group(inner) => self.eval_nat_expr(inner, variant_name),
            HirExprNode::Ident(name) => self.eval_ident(expr, name, variant_name),
            HirExprNode::Field { .. } => self.eval_enum_variant(expr, variant_name),
            HirExprNode::Binary {
                op, left, right, ..
            } => self.eval_binary(
                expr,
                &BinaryExpr {
                    op: *op,
                    left,
                    right,
                },
                variant_name,
            ),
            _ => Err(self.nat_error(expr.span(), variant_name)),
        }
    }

    fn eval_binary(
        &mut self,
        expr: &HirBodyExpr,
        args: &BinaryExpr<'_>,
        variant_name: &str,
    ) -> Result<u64, CompileError> {
        let lhs = self.eval_nat_expr(args.left, variant_name)?;
        let rhs = self.eval_nat_expr(args.right, variant_name)?;
        let value = match args.op {
            BinaryOp::Add => lhs.checked_add(rhs),
            BinaryOp::Sub => Some(lhs.saturating_sub(rhs)),
            BinaryOp::Mul => lhs.checked_mul(rhs),
            BinaryOp::Div if rhs != 0 => Some(lhs / rhs),
            BinaryOp::Rem if rhs != 0 => Some(lhs % rhs),
            BinaryOp::Shl => lhs.checked_shl(
                u32::try_from(rhs).map_err(|_| self.nat_error(expr.span(), variant_name))?,
            ),
            _ => None,
        };
        value.ok_or_else(|| self.nat_error(expr.span(), variant_name))
    }

    fn eval_ident(
        &mut self,
        expr: &HirBodyExpr,
        name: &str,
        variant_name: &str,
    ) -> Result<u64, CompileError> {
        if let Some(value) = self.values_by_name.get(name).copied() {
            return Ok(value);
        }
        let Some(HirResolution::Def(def)) = self
            .checker
            .hir
            .expr_resolution(self.owner, expr)
            .ok()
            .flatten()
        else {
            return Err(self.nat_error(expr.span(), variant_name));
        };
        let Some(item) = self.checker.hir.const_by_def(def) else {
            return Err(self.nat_error(expr.span(), variant_name));
        };
        self.eval_const_item(def, item, variant_name)
    }

    fn eval_enum_variant(
        &mut self,
        expr: &HirBodyExpr,
        variant_name: &str,
    ) -> Result<u64, CompileError> {
        let Some((enum_def, variant)) = self.checker.hir.enum_variant_expr(expr) else {
            return Err(self.nat_error(expr.span(), variant_name));
        };
        if enum_def == self.owner {
            if let Some(value) = self.values_by_name.get(variant).copied() {
                return Ok(value);
            }
        }
        let key = HirEnumVariantKey::new(enum_def, variant);
        if let Some(value) = self.checker.enum_variant_values.get(&key).copied() {
            return Ok(value);
        }
        Err(self.nat_error(expr.span(), variant_name))
    }

    fn eval_const_item(
        &mut self,
        def: DefId,
        item: &HirConstItem,
        variant_name: &str,
    ) -> Result<u64, CompileError> {
        if !self.const_stack.insert(def) {
            return Err(self.nat_error(item.value.span(), variant_name));
        }
        let value = self.eval_nat_expr(&item.value, variant_name);
        self.const_stack.remove(&def);
        value
    }

    fn nat_error(&self, span: syl_span::Span, variant_name: &str) -> CompileError {
        CompileError::lowering_at(
            TirError::RequiresNatExpression {
                context: format!("enum discriminant {}.{}", self.item.name, variant_name),
            },
            span,
        )
    }
}

fn enum_width_value(
    checker: &TypePhaseChecker,
    owner: DefId,
    ty: &MirTypeRef,
    enum_name: &str,
) -> Result<u64, CompileError> {
    let tir = checker.type_from_mir_type_ref(owner, ty)?;
    let width = match tir {
        TirType::UInt { width } | TirType::Bits { width } | TirType::SInt { width } => width,
        _ => {
            return Err(CompileError::lowering_at(
                TirError::RequiresNatExpression {
                    context: format!("enum width for {enum_name}"),
                },
                ty.span(),
            ));
        }
    };
    match width {
        TirConstTerm::NatLiteral(value) => Ok(value),
        _ => Err(CompileError::lowering_at(
            TirError::RequiresNatExpression {
                context: format!("enum width for {enum_name}"),
            },
            ty.span(),
        )),
    }
}

fn one_hot_value(index: usize) -> Option<u64> {
    let shift = u32::try_from(index).ok()?;
    1u64.checked_shl(shift)
}

fn fits_width(width: u64, value: u64) -> bool {
    if width >= u64::from(u64::BITS) {
        true
    } else {
        let shift = u32::try_from(width).unwrap_or(u32::MAX);
        value < (1u64 << shift)
    }
}
