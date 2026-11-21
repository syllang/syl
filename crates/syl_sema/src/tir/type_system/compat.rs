use super::const_term::TirConstResolution;
use super::{TirConstTerm, TirGenericArg, TirType, TypePhaseChecker};
use crate::hir::view::HirDesignViewExt;
use crate::tir::consts::TirConstEnv;
use crate::{CompileError, TirError, hir::HirBodyExpr};
use std::collections::BTreeMap;
use syl_hir::LocalId;

pub(in crate::tir) fn is_int_literal(expr: &HirBodyExpr) -> bool {
    match &expr.node {
        crate::hir::HirExprNode::Int(_) => true,
        crate::hir::HirExprNode::Group(inner) => is_int_literal(inner),
        crate::hir::HirExprNode::Match { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| is_int_literal(&arm.value))
        }
        crate::hir::HirExprNode::Select { arms, .. } => {
            !arms.is_empty() && arms.iter().all(|arm| is_int_literal(&arm.value))
        }
        _ => false,
    }
}

impl TypePhaseChecker {
    pub(in crate::tir) fn types_compatible_for_return(
        &self,
        expected: &TirType,
        actual: &TirType,
        int_literal: bool,
    ) -> bool {
        use TirType::*;
        if matches!(expected, Unknown) || matches!(actual, Unknown) || expected == actual {
            return true;
        }
        if int_literal && matches!(actual, Nat) {
            return matches!(
                expected,
                Nat | Bit | UInt { .. } | SInt { .. } | Bits { .. }
            );
        }
        match (expected, actual) {
            (UInt { width: a }, UInt { width: b })
            | (Bits { width: a }, Bits { width: b })
            | (SInt { width: a }, SInt { width: b }) => self.const_terms_compatible(a, b),
            (Array { len: a, elem: ae }, Array { len: b, elem: be }) => {
                self.const_terms_compatible(a, b) && self.types_compatible_for_return(ae, be, false)
            }
            (View { base: a, view: av }, View { base: b, view: bv }) => {
                av == bv && self.types_compatible_for_return(a, b, false)
            }
            (
                Named {
                    def: a,
                    generic: ag,
                    args: aa,
                    ..
                },
                Named {
                    def: b,
                    generic: bg,
                    args: ba,
                    ..
                },
            ) => {
                ((a.is_some() && a == b) || (ag.is_some() && ag == bg))
                    && aa.len() == ba.len()
                    && aa.iter().zip(ba).all(|(a, b)| match (a, b) {
                        (TirGenericArg::Type(a), TirGenericArg::Type(b)) => {
                            self.types_compatible_for_return(a, b, false)
                        }
                        (TirGenericArg::Const(a), TirGenericArg::Const(b)) => {
                            self.const_terms_compatible(a, b)
                        }
                        _ => false,
                    })
            }
            _ => false,
        }
    }

    pub(in crate::tir) fn expr_compatible_for_return(
        &self,
        expected: &TirType,
        expr: &HirBodyExpr,
    ) -> bool {
        use crate::hir::HirExprNode;
        match &expr.node {
            HirExprNode::Group(inner) => self.expr_compatible_for_return(expected, inner),
            HirExprNode::Match { arms, .. } => arms
                .iter()
                .all(|arm| self.expr_compatible_for_return(expected, &arm.value)),
            HirExprNode::Select { arms, .. } => arms
                .iter()
                .all(|arm| self.expr_compatible_for_return(expected, &arm.value)),
            _ => self.current_owner().is_ok_and(|owner| {
                self.types_compatible_for_return(
                    expected,
                    &self.infer_expr_type(owner, expr),
                    is_int_literal(expr),
                )
            }),
        }
    }

    fn const_terms_compatible(&self, a: &TirConstTerm, b: &TirConstTerm) -> bool {
        let a = self.evaluated_const_term(a);
        let b = self.evaluated_const_term(b);
        a == b
            || matches!(a, TirConstTerm::Unknown | TirConstTerm::Expr { .. })
            || matches!(b, TirConstTerm::Unknown | TirConstTerm::Expr { .. })
    }

    fn evaluated_const_term(&self, term: &TirConstTerm) -> TirConstTerm {
        if let TirConstTerm::Named {
            resolution: Some(TirConstResolution::Def(def)),
            ..
        } = term
            && let Some(item) = self.hir.consts().get(def)
            && let Some(value) =
                TirConstEnv::from_generics(*def, &[], self).const_nat_value(&item.value, self)
        {
            return TirConstTerm::NatLiteral(value);
        }
        term.clone()
    }

    pub(in crate::tir) fn check_branch_types<'a>(
        &self,
        values: impl IntoIterator<Item = &'a HirBodyExpr>,
        errors: &mut Vec<CompileError>,
    ) {
        let Ok(owner) = self.current_owner() else {
            return;
        };
        let values = values
            .into_iter()
            .map(|expr| (expr, self.infer_expr_type(owner, expr)))
            .collect::<Vec<_>>();
        // Prefer a typed arm over an integer literal, whose hardware type is contextual.
        let expected = values
            .iter()
            .find(|(expr, ty)| !is_int_literal(expr) && !matches!(ty, TirType::Unknown))
            .or_else(|| {
                values
                    .iter()
                    .find(|(_, ty)| !matches!(ty, TirType::Unknown))
            });
        let Some((_, expected)) = expected else {
            return;
        };
        for (expr, actual) in &values {
            if !self.types_compatible_for_return(expected, actual, is_int_literal(expr)) {
                errors.push(CompileError::lowering_at(
                    TirError::TypeMismatch {
                        expected: expected.label(),
                        actual: actual.label(),
                    },
                    expr.span(),
                ));
            }
        }
    }

    pub(in crate::tir) fn field_type_for(&self, base: &TirType, field: &str) -> Option<TirType> {
        use crate::hir::view::HirDesignViewExt;
        let def = base.definition()?;
        let field_ty = self
            .hir
            .member_field_type(def, base.selected_view(), field)?;
        let ty = self.type_from_mir_type_ref(def, &field_ty).ok()?;
        let generics = self
            .hir
            .structs()
            .get(&def)
            .map(|item| &item.generics)
            .or_else(|| self.hir.bundles().get(&def).map(|item| &item.generics))
            .or_else(|| self.hir.interfaces().get(&def).map(|item| &item.generics))?;
        let bindings = generics
            .iter()
            .zip(base.generic_args())
            .filter_map(|(generic, arg)| Some((generic.id?, arg.clone())))
            .collect();
        Some(ty.substitute(&bindings))
    }
}

impl TirType {
    pub(in crate::tir) fn substitute(&self, bindings: &BTreeMap<LocalId, TirGenericArg>) -> Self {
        match self {
            Self::Named {
                generic: Some(id), ..
            } if bindings.contains_key(id) => match &bindings[id] {
                TirGenericArg::Type(ty) => (**ty).clone(),
                _ => self.clone(),
            },
            Self::Named {
                name,
                def,
                generic,
                kind,
                args,
            } => Self::Named {
                name: name.clone(),
                def: *def,
                generic: *generic,
                kind: *kind,
                args: args
                    .iter()
                    .map(|arg| match arg {
                        TirGenericArg::Type(ty) => {
                            TirGenericArg::Type(Box::new(ty.substitute(bindings)))
                        }
                        TirGenericArg::Const(term) => {
                            TirGenericArg::Const(term.substitute(bindings))
                        }
                    })
                    .collect(),
            },
            Self::UInt { width } => Self::UInt {
                width: width.substitute(bindings),
            },
            Self::Bits { width } => Self::Bits {
                width: width.substitute(bindings),
            },
            Self::SInt { width } => Self::SInt {
                width: width.substitute(bindings),
            },
            Self::Array { len, elem } => Self::Array {
                len: len.substitute(bindings),
                elem: Box::new(elem.substitute(bindings)),
            },
            Self::View { base, view } => Self::View {
                base: Box::new(base.substitute(bindings)),
                view: view.clone(),
            },
            Self::Clock { domain } => Self::Clock {
                domain: domain.as_ref().map(|d| Box::new(d.substitute(bindings))),
            },
            Self::Reset { domain } => Self::Reset {
                domain: domain.as_ref().map(|d| Box::new(d.substitute(bindings))),
            },
            _ => self.clone(),
        }
    }
}

impl TirConstTerm {
    fn substitute(&self, bindings: &BTreeMap<LocalId, TirGenericArg>) -> Self {
        if let Self::Named {
            resolution: Some(TirConstResolution::Local(id)),
            ..
        } = self
            && let Some(TirGenericArg::Const(term)) = bindings.get(id)
        {
            return term.clone();
        }
        self.clone()
    }
}

impl TypePhaseChecker {
    pub(in crate::tir) fn infer_branch_type<'a>(
        &self,
        owner: syl_hir::DefId,
        values: impl Iterator<Item = &'a HirBodyExpr>,
    ) -> TirType {
        let mut literal = TirType::Unknown;
        for expr in values {
            let ty = self.infer_expr_type(owner, expr);
            if !matches!(ty, TirType::Unknown) {
                if !is_int_literal(expr) {
                    return ty;
                }
                literal = ty;
            }
        }
        literal
    }

    pub(super) fn declared_def_type(
        &self,
        def: syl_hir::DefId,
        visited: &mut std::collections::BTreeSet<syl_hir::DefId>,
    ) -> Option<TirType> {
        if let Some(ty) = self.binding_types.get(&crate::tir::BindingRef::Def(def)) {
            return self.type_table.get(*ty).cloned();
        }
        if !visited.insert(def) {
            return None;
        }
        let result = if let Some(item) = self.hir.fns().get(&def) {
            item.ret_ty
                .as_ref()
                .and_then(|ty| self.type_from_mir_type_ref(def, &ty.ty).ok())
        } else if let Some(item) = self.hir.maps().get(&def) {
            item.ret_ty
                .as_ref()
                .and_then(|ty| self.type_from_mir_type_ref(def, &ty.ty).ok())
        } else if let Some(item) = self.hir.consts().get(&def) {
            if let Some(ty) = &item.ty {
                self.type_from_mir_type_ref(def, ty).ok()
            } else {
                self.const_decl_expr_type(def, &item.value, visited)
            }
        } else {
            None
        };
        visited.remove(&def);
        result
    }
    fn const_decl_expr_type(
        &self,
        owner: syl_hir::DefId,
        expr: &HirBodyExpr,
        visited: &mut std::collections::BTreeSet<syl_hir::DefId>,
    ) -> Option<TirType> {
        use crate::hir::{HirExprNode, resolve::HirResolution};
        match &expr.node {
            HirExprNode::Int(_) => Some(TirType::Nat),
            HirExprNode::Bool(_) => Some(TirType::Bool),
            HirExprNode::Str(_) => Some(TirType::Str),
            HirExprNode::Ident(_) => match self.hir.expr_resolution(owner, expr).ok().flatten()? {
                HirResolution::Def(def) => self.declared_def_type(def, visited),
                _ => None,
            },
            HirExprNode::Group(inner) => self.const_decl_expr_type(owner, inner, visited),
            HirExprNode::Unary {
                op: syl_syntax::UnaryOp::Not,
                ..
            } => Some(TirType::Bool),
            HirExprNode::Unary { .. } => Some(TirType::Nat),
            HirExprNode::Binary { op, .. } => Some(if self.binary_returns_bool(*op) {
                TirType::Bool
            } else {
                TirType::Nat
            }),
            HirExprNode::Aggregate { ty, .. } => self.type_from_mir_type_ref(owner, ty).ok(),
            HirExprNode::Call { callee, .. } => self.const_decl_expr_type(owner, callee, visited),
            HirExprNode::Field { base, field } => {
                self.field_type_for(&self.const_decl_expr_type(owner, base, visited)?, field)
            }
            _ => None,
        }
    }
}
