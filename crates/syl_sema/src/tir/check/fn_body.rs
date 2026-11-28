use crate::hir::view::HirDesignViewExt;
mod calls;
mod expr;
mod flow;

use super::super::{BindingKind, BindingRef, Phase, TirType, TypePhaseChecker};
use crate::hir::resolve::HirResolution;
use crate::tir::type_system::compat::is_int_literal;
use crate::{
    CompileError, TirError,
    hir::{HirBlock, HirBodyExpr, HirExprNode, HirFnItem, HirStmt},
    ir::mir::MirTypeRef,
};
use std::collections::BTreeMap;
use syl_hir::{DefId, LocalId};
use syl_span::Span;

#[derive(Clone)]
struct FnLocalInfo {
    ty: TirType,
    mutable: bool,
}

#[derive(Clone, Default)]
struct FnTypeEnv {
    locals: BTreeMap<LocalId, FnLocalInfo>,
    return_type: Option<TirType>,
}

struct FnLocalDecl<'a> {
    id: Option<LocalId>,
    name: &'a str,
    ty: Option<&'a MirTypeRef>,
    value: Option<&'a HirBodyExpr>,
    span: Span,
    kind: BindingKind,
    mutable: bool,
}

impl TypePhaseChecker {
    pub(super) fn check_format_const_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.check_fn_expr(expr, &FnTypeEnv::default(), errors)
            .map(|_| ())
    }

    pub(super) fn check_fn_body(
        &mut self,
        owner: DefId,
        item: &HirFnItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        if item.body.stmts.is_empty()
            && item.body.tail.is_none()
            && item.body.span.start == item.body.span.end
        {
            return Ok(());
        }
        let mut env = FnTypeEnv {
            return_type: item.ret_ty.as_ref().map(|ret| {
                self.type_from_mir_type_ref(owner, &ret.ty)
                    .unwrap_or(TirType::Unknown)
            }),
            ..FnTypeEnv::default()
        };
        for param in &item.params {
            if let Some(id) = param.id {
                let ty = self
                    .type_from_mir_type_ref(owner, &param.ty)
                    .unwrap_or(TirType::Unknown);
                env.locals.insert(id, FnLocalInfo { ty, mutable: false });
            }
        }
        let returns = self.check_fn_block(&item.body, &mut env, errors)?;
        if let Some(expected) = &env.return_type {
            let value_tail = item.body.tail.as_ref().filter(|tail| {
                !Self::fn_expr_definitely_returns(tail) && !Self::fn_expr_has_void_tail_block(tail)
            });
            if let Some(tail) = value_tail {
                let actual = self.fn_expr_type(tail);
                self.check_fn_return(Some(expected), Some((tail, actual)), tail.span(), errors);
            } else if !returns {
                errors.push(CompileError::lowering_at(
                    TirError::MissingReturnValue {
                        name: item.name.clone(),
                        expected: expected.label(),
                    },
                    item.body.span,
                ));
            }
        } else if let Some(tail) = &item.body.tail
            && !self.fn_expr_is_void(tail)
        {
            let actual = self.fn_expr_type(tail);
            self.check_fn_return(None, Some((tail, actual)), tail.span(), errors);
        }
        Ok(())
    }

    fn check_fn_block(
        &mut self,
        block: &HirBlock,
        env: &mut FnTypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<bool, CompileError> {
        self.checked_blocks += 1;
        let mut returns = false;
        for stmt in &block.stmts {
            // Continue checking unreachable statements to retain IDE facts and diagnostics.
            returns |= self.check_fn_stmt(stmt, env, errors)?;
        }
        if let Some(tail) = &block.tail {
            self.check_fn_expr(tail, env, errors)?;
            returns |= Self::fn_expr_definitely_returns(tail);
        }
        Ok(returns)
    }

    fn check_fn_stmt(
        &mut self,
        stmt: &HirStmt,
        env: &mut FnTypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<bool, CompileError> {
        match stmt {
            HirStmt::Const {
                id,
                name,
                ty,
                value,
                span,
            } => self.check_fn_local(
                FnLocalDecl {
                    id: *id,
                    name,
                    ty: ty.as_ref(),
                    value: Some(value),
                    span: *span,
                    kind: BindingKind::Const,
                    mutable: false,
                },
                env,
                errors,
            )?,
            HirStmt::Let {
                id,
                name,
                ty,
                value,
                span,
            }
            | HirStmt::Var {
                id,
                name,
                ty,
                value,
                span,
            } => {
                self.check_fn_local(
                    FnLocalDecl {
                        id: *id,
                        name,
                        ty: ty.as_ref(),
                        value: value.as_ref(),
                        span: *span,
                        kind: BindingKind::Local,
                        mutable: matches!(stmt, HirStmt::Var { .. }),
                    },
                    env,
                    errors,
                )?;
            }
            HirStmt::Assign { target, value, .. } => {
                let actual = self.check_fn_expr(value, env, errors)?;
                let expected = self.check_fn_expr(target, env, errors)?;
                if let Some((id, name)) = self.fn_assignment_root(target) {
                    if !env.locals.get(&id).is_some_and(|local| local.mutable) {
                        errors.push(CompileError::lowering_at(
                            TirError::ImmutableAssignment { name },
                            target.span(),
                        ));
                    }
                } else {
                    errors.push(CompileError::lowering_at(
                        TirError::InvalidElaborationExpression,
                        target.span(),
                    ));
                }
                self.check_fn_type(&expected, &actual, value, errors);
            }
            HirStmt::ElabIf {
                cond,
                then_block,
                else_block,
                ..
            } => {
                let ty = self.check_fn_expr(cond, env, errors)?;
                self.check_fn_type(&TirType::Bool, &ty, cond, errors);
                let then_returns = self.check_fn_block(then_block, &mut env.clone(), errors)?;
                let else_returns = if let Some(block) = else_block {
                    self.check_fn_block(block, &mut env.clone(), errors)?
                } else {
                    false
                };
                return Ok(then_returns && else_returns);
            }
            HirStmt::While { cond, body, .. } => {
                let ty = self.check_fn_expr(cond, env, errors)?;
                self.check_fn_type(&TirType::Bool, &ty, cond, errors);
                self.check_fn_block(body, &mut env.clone(), errors)?;
            }
            HirStmt::ElabFor {
                id,
                name,
                range,
                body,
                span,
            } => {
                self.check_fn_range(range, env, errors)?;
                let mut nested = env.clone();
                self.check_fn_local(
                    FnLocalDecl {
                        id: *id,
                        name,
                        ty: None,
                        value: None,
                        span: *span,
                        kind: BindingKind::Local,
                        mutable: false,
                    },
                    &mut nested,
                    errors,
                )?;
                if let Some(id) = id {
                    nested.locals.insert(
                        *id,
                        FnLocalInfo {
                            ty: TirType::Nat,
                            mutable: false,
                        },
                    );
                    self.record_binding_type(BindingRef::Local(*id), TirType::Nat);
                }
                self.check_fn_block(body, &mut nested, errors)?;
            }
            HirStmt::Return(value, span) => {
                let actual = value
                    .as_ref()
                    .map(|expr| self.check_fn_expr(expr, env, errors).map(|ty| (expr, ty)))
                    .transpose()?;
                self.check_fn_return(env.return_type.as_ref(), actual, *span, errors);
                return Ok(true);
            }
            HirStmt::Expr(expr) => {
                self.check_fn_expr(expr, env, errors)?;
                return Ok(Self::fn_expr_definitely_returns(expr));
            }
            HirStmt::Signal { span, .. }
            | HirStmt::Reg { span, .. }
            | HirStmt::Mem { span, .. }
            | HirStmt::Drive { span, .. }
            | HirStmt::Next { span, .. } => {
                errors.push(CompileError::lowering_at(
                    TirError::HardwareStatementInFn,
                    *span,
                ));
            }
            HirStmt::Error { .. } => {}
            _ => {}
        }
        Ok(false)
    }

    fn check_fn_local(
        &mut self,
        decl: FnLocalDecl<'_>,
        env: &mut FnTypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let FnLocalDecl {
            id,
            name,
            ty,
            value,
            span,
            kind,
            mutable,
        } = decl;
        Self::record_recoverable(errors, self.record_decl_local_binding(name, id, span, kind));
        let explicit = ty.and_then(|ty| {
            Self::record_recoverable(
                errors,
                self.type_from_mir_type_ref(self.current_owner().ok()?, ty),
            )
        });
        let actual = value
            .map(|value| self.check_fn_expr(value, env, errors))
            .transpose()?
            .unwrap_or(TirType::Unknown);
        if let (Some(expected), Some(value)) = (&explicit, value) {
            self.check_fn_type(expected, &actual, value, errors);
        }
        if explicit.is_none()
            && let Some(value) = value
            && self.fn_expr_is_void(value)
        {
            errors.push(CompileError::lowering_at(
                TirError::TypeMismatch {
                    expected: "value".to_string(),
                    actual: "void".to_string(),
                },
                value.span(),
            ));
        }
        let ty = explicit.unwrap_or(actual);
        Self::record_recoverable(
            errors,
            self.record_decl_local_type(name, id, span, ty.clone()),
        );
        if let Some(id) = id {
            env.locals.insert(id, FnLocalInfo { ty, mutable });
        }
        Ok(())
    }

    fn check_fn_return(
        &self,
        expected: Option<&TirType>,
        actual: Option<(&HirBodyExpr, TirType)>,
        span: Span,
        errors: &mut Vec<CompileError>,
    ) {
        let compatible = match (expected, &actual) {
            (Some(expected), Some((expr, _))) => self.fn_return_expr_compatible(expected, expr),
            (None, None) => true,
            _ => false,
        };
        if !compatible {
            errors.push(CompileError::lowering_at(
                TirError::ReturnTypeMismatch {
                    expected: expected.map_or_else(|| "void".to_string(), TirType::label),
                    actual: actual.as_ref().map_or_else(
                        || "void".to_string(),
                        |(expr, ty)| {
                            if self.fn_expr_is_void(expr) {
                                "void".to_string()
                            } else {
                                ty.label()
                            }
                        },
                    ),
                },
                actual.map_or(span, |(expr, _)| expr.span()),
            ));
        }
    }

    fn fn_return_expr_compatible(&self, expected: &TirType, expr: &HirBodyExpr) -> bool {
        if self.fn_expr_is_void(expr) {
            return false;
        }
        match &expr.node {
            HirExprNode::Group(inner) => self.fn_return_expr_compatible(expected, inner),
            HirExprNode::Match { arms, .. } => arms
                .iter()
                .all(|arm| self.fn_return_expr_compatible(expected, &arm.value)),
            HirExprNode::Block(block) if block.tail.is_some() => {
                self.fn_return_expr_compatible(expected, block.tail.as_ref().unwrap())
            }
            _ => self.types_compatible_for_return(
                expected,
                &self.fn_expr_type(expr),
                is_int_literal(expr),
            ),
        }
    }

    fn check_fn_type(
        &self,
        expected: &TirType,
        actual: &TirType,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) {
        if self.fn_expr_is_void(expr)
            || !self.types_compatible_for_return(expected, actual, is_int_literal(expr))
        {
            errors.push(CompileError::lowering_at(
                TirError::TypeMismatch {
                    expected: expected.label(),
                    actual: if self.fn_expr_is_void(expr) {
                        "void".to_string()
                    } else {
                        actual.label()
                    },
                },
                expr.span(),
            ));
        }
    }

    fn check_fn_range(
        &mut self,
        range: &HirBodyExpr,
        env: &FnTypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        if let HirExprNode::Range { start, end } = &range.node {
            for (expr, context) in [(start, "for range start"), (end, "for range end")] {
                let ty = self.check_fn_expr(expr, env, errors)?;
                if !matches!(ty, TirType::Nat | TirType::Unknown) {
                    errors.push(CompileError::lowering_at(
                        TirError::RequiresNatExpression {
                            context: context.to_string(),
                        },
                        expr.span(),
                    ));
                }
            }
            self.record_phase(range, Phase::Const)?;
        } else {
            self.check_fn_expr(range, env, errors)?;
            errors.push(CompileError::lowering_at(
                TirError::RequiresNatExpression {
                    context: "for range".to_string(),
                },
                range.span(),
            ));
        }
        Ok(())
    }

    fn fn_assignment_root(&self, expr: &HirBodyExpr) -> Option<(LocalId, String)> {
        match &expr.node {
            HirExprNode::Ident(name) => match self
                .hir
                .expr_resolution(self.current_owner().ok()?, expr)
                .ok()
                .flatten()?
            {
                HirResolution::Local(id) => Some((id, name.clone())),
                _ => None,
            },
            HirExprNode::Field { base, .. } | HirExprNode::Group(base) => {
                self.fn_assignment_root(base)
            }
            _ => None,
        }
    }
}
