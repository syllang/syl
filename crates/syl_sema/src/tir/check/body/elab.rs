use crate::{
    CompileError, EirError, TirError,
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{HirBlock, HirBodyExpr, HirExprNode, HirStmt},
    tir::{
        BindingKind, HardwareBlockMode, Phase, TirType, TypePhaseChecker,
        consts::{TirConstEnv, TirConstKind},
    },
};
use syl_hir::{LocalId, MirTypeRef};
use syl_span::Span;

#[derive(Clone, Copy)]
pub(super) struct ElabForLoop<'a> {
    pub(super) id: Option<LocalId>,
    pub(super) name: &'a str,
    pub(super) span: Span,
}

pub(super) struct HardwareCheckContext<'a> {
    pub(super) env: &'a TirConstEnv,
    pub(super) mode: HardwareBlockMode,
    pub(super) errors: &'a mut Vec<CompileError>,
}

#[derive(Clone, Copy)]
pub(super) struct ElabVarStmt<'a> {
    pub(super) id: Option<LocalId>,
    pub(super) name: &'a str,
    pub(super) ty: Option<&'a MirTypeRef>,
    pub(super) value: Option<&'a HirBodyExpr>,
    pub(super) span: Span,
}

#[derive(Clone, Copy)]
pub(super) struct ElabAssignmentStmt<'a> {
    pub(super) target: &'a HirBodyExpr,
    pub(super) value: &'a HirBodyExpr,
    pub(super) span: Span,
}

impl TypePhaseChecker {
    pub(super) fn check_elab_if(
        &mut self,
        cond: &HirBodyExpr,
        then_block: &HirBlock,
        else_block: Option<&HirBlock>,
        context: HardwareCheckContext<'_>,
    ) -> Result<TirConstEnv, CompileError> {
        Self::record_recoverable(context.errors, self.record_phase(cond, Phase::Const));
        Self::record_recoverable(context.errors, self.require_const_bool(cond, context.env));
        match context.env.const_bool_value(cond, self) {
            Some(true) => {
                self.check_hardware_block(then_block, context.env, context.mode, context.errors)
            }
            Some(false) => {
                if let Some(block) = else_block {
                    return self.check_hardware_block(
                        block,
                        context.env,
                        context.mode,
                        context.errors,
                    );
                }
                Ok(context.env.clone())
            }
            None => {
                let then_env = self.check_control_block(then_block, context.env, context.errors)?;
                if let Some(block) = else_block {
                    let else_env = self.check_control_block(block, context.env, context.errors)?;
                    Ok(context.env.merge_branch_mutations(&then_env, &else_env))
                } else {
                    Ok(context.env.merge_visible_mutations_from(&then_env))
                }
            }
        }
    }

    pub(super) fn check_elab_for(
        &mut self,
        loop_decl: ElabForLoop<'_>,
        range: &HirBodyExpr,
        body: &HirBlock,
        context: HardwareCheckContext<'_>,
    ) -> Result<TirConstEnv, CompileError> {
        let loop_id = Self::record_recoverable(
            context.errors,
            self.record_decl_local_binding(
                loop_decl.name,
                loop_decl.id,
                loop_decl.span,
                BindingKind::Const,
            ),
        );
        Self::record_recoverable(
            context.errors,
            self.record_decl_local_type(loop_decl.name, loop_decl.id, loop_decl.span, TirType::Nat),
        );
        Self::record_recoverable(context.errors, self.record_phase(range, Phase::Const));
        Self::record_recoverable(context.errors, self.require_const_range(range, context.env));
        let loop_env = if let Some(loop_id) = loop_id {
            context
                .env
                .with_local_binding(loop_id, TirConstKind::Nat, None)
        } else {
            context.env.clone()
        };
        match context.env.const_range_bounds(range, self) {
            Some((start, end)) if start >= end => Ok(context.env.clone()),
            Some(_) => {
                let nested =
                    self.check_hardware_block(body, &loop_env, context.mode, context.errors)?;
                Ok(context.env.apply_visible_mutations_from(&nested))
            }
            None => {
                let nested = self.check_control_block(body, &loop_env, context.errors)?;
                Ok(context.env.merge_visible_mutations_from(&nested))
            }
        }
    }

    pub(super) fn check_elab_var_stmt(
        &mut self,
        stmt: ElabVarStmt<'_>,
        env: &TirConstEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<TirConstEnv, CompileError> {
        let local_id =
            self.record_decl_local_binding(stmt.name, stmt.id, stmt.span, BindingKind::Local)?;
        let explicit_ty = if let Some(ty) = stmt.ty {
            Self::record_recoverable(
                errors,
                self.type_from_mir_type_ref(self.current_owner()?, ty),
            )
        } else {
            None
        };
        if let Some(explicit_ty) = explicit_ty.clone() {
            Self::record_recoverable(
                errors,
                self.record_decl_local_type(stmt.name, Some(local_id), stmt.span, explicit_ty),
            );
        }
        if let Some(explicit_ty) = explicit_ty.clone()
            && let Some(value) = stmt.value
        {
            Self::record_recoverable(errors, self.record_expr_type(value, explicit_ty));
        }
        let scalar_kind = explicit_ty
            .as_ref()
            .and_then(tir_const_kind_for_type)
            .or_else(|| {
                explicit_ty
                    .is_none()
                    .then(|| stmt.value.and_then(|expr| env.expr_kind(expr, self)))
                    .flatten()
            });
        let struct_def = explicit_ty
            .as_ref()
            .and_then(TirType::definition)
            .filter(|def| self.hir().structs.contains_key(def))
            .or_else(|| {
                explicit_ty
                    .is_none()
                    .then(|| {
                        stmt.value
                            .and_then(|expr| env.struct_def_for_expr(expr, self))
                    })
                    .flatten()
            });
        if scalar_kind.is_none() && struct_def.is_none() {
            errors.push(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                stmt.span,
            ));
            return Ok(env.clone());
        }
        if explicit_ty.is_none()
            && let Some(kind) = scalar_kind
        {
            Self::record_recoverable(
                errors,
                self.record_decl_local_type(
                    stmt.name,
                    Some(local_id),
                    stmt.span,
                    tir_type_for_const_kind(kind),
                ),
            );
        }
        if let Some(expr) = stmt.value {
            Self::record_recoverable(errors, self.record_phase(expr, Phase::Const));
            if let Some(kind) = scalar_kind {
                self.require_const_expr_kind(expr, env, kind, errors);
            }
        }
        if let Some(kind) = scalar_kind {
            return Ok(env.with_mutable_local(
                local_id,
                kind,
                stmt.value
                    .and_then(|expr| env.value_for_kind(kind, expr, self)),
            ));
        }
        let Some(def) = struct_def else {
            return Ok(env.clone());
        };
        Ok(env.with_mutable_struct_local(
            local_id,
            def,
            stmt.value
                .and_then(|expr| env.struct_value_for_expr(expr, self)),
        ))
    }

    pub(super) fn check_elab_assignment(
        &mut self,
        stmt: ElabAssignmentStmt<'_>,
        env: &TirConstEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<Option<TirConstEnv>, CompileError> {
        let owner = self.current_owner()?;
        let target_local = match &stmt.target.node {
            HirExprNode::Ident(_) => self.hir.expr_resolution(owner, stmt.target),
            HirExprNode::Field { base, .. } => self.hir.expr_resolution(owner, base),
            _ => return Ok(None),
        };
        let Ok(Some(HirResolution::Local(id))) = target_local else {
            return Ok(None);
        };
        if !env.is_mutable_local(id) {
            return Ok(None);
        }
        Self::record_recoverable(errors, self.record_phase(stmt.target, Phase::Const));
        Self::record_recoverable(errors, self.record_phase(stmt.value, Phase::Const));
        match &stmt.target.node {
            HirExprNode::Ident(_) => {
                let Some(kind) = env.kind_for_local(id) else {
                    return Ok(None);
                };
                self.require_const_expr_kind(stmt.value, env, kind, errors);
                Ok(Some(
                    env.assign_local(id, env.value_for_kind(kind, stmt.value, self))
                        .unwrap_or_else(|| env.clone()),
                ))
            }
            HirExprNode::Field { base: _, field } => {
                let Some(struct_def) = env.struct_def_for_local(id) else {
                    return Ok(None);
                };
                let Some(field_kind) = self
                    .hir
                    .member_field_type(struct_def, None, field)
                    .and_then(|field_ty| self.mir_type_kind(&field_ty))
                else {
                    errors.push(CompileError::lowering_at(
                        TirError::InvalidElaborationExpression,
                        stmt.span,
                    ));
                    return Ok(Some(env.clone()));
                };
                self.require_const_expr_kind(stmt.value, env, field_kind, errors);
                Ok(Some(
                    env.assign_field(id, field, env.value_for_kind(field_kind, stmt.value, self))
                        .unwrap_or_else(|| env.clone()),
                ))
            }
            _ => {
                errors.push(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    stmt.span,
                ));
                Ok(Some(env.clone()))
            }
        }
    }

    fn check_control_block(
        &mut self,
        body: &HirBlock,
        env: &TirConstEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<TirConstEnv, CompileError> {
        let mut nested = env.clone();
        self.checked_blocks += 1;
        for stmt in &body.stmts {
            match stmt {
                HirStmt::While { span, .. } | HirStmt::Return(_, span) => errors.push(
                    CompileError::lowering_at(EirError::IllegalHardwareStatement, *span),
                ),
                HirStmt::Var {
                    id,
                    name,
                    ty,
                    value,
                    span,
                } => {
                    nested = self.check_elab_var_stmt(
                        ElabVarStmt {
                            id: *id,
                            name,
                            ty: ty.as_ref(),
                            value: value.as_ref(),
                            span: *span,
                        },
                        &nested,
                        errors,
                    )?;
                }
                HirStmt::Assign {
                    target,
                    value,
                    span,
                } => {
                    if let Some(updated_env) = self.check_elab_assignment(
                        ElabAssignmentStmt {
                            target,
                            value,
                            span: *span,
                        },
                        &nested,
                        errors,
                    )? {
                        nested = updated_env;
                    } else {
                        errors.push(CompileError::lowering_at(
                            EirError::IllegalHardwareStatement,
                            *span,
                        ));
                    }
                }
                HirStmt::ElabIf {
                    cond,
                    then_block,
                    else_block,
                    ..
                } => {
                    nested = self.check_elab_if(
                        cond,
                        then_block,
                        else_block.as_ref(),
                        HardwareCheckContext {
                            env: &nested,
                            mode: HardwareBlockMode::Control,
                            errors,
                        },
                    )?;
                }
                HirStmt::ElabFor {
                    id,
                    name,
                    range,
                    body,
                    span,
                    ..
                } => {
                    nested = self.check_elab_for(
                        ElabForLoop {
                            id: *id,
                            name,
                            span: *span,
                        },
                        range,
                        body,
                        HardwareCheckContext {
                            env: &nested,
                            mode: HardwareBlockMode::Control,
                            errors,
                        },
                    )?;
                }
                HirStmt::Const {
                    id,
                    name,
                    ty,
                    value,
                    span,
                    ..
                } => {
                    let local_id = Self::record_recoverable(
                        errors,
                        self.record_decl_local_binding(name, *id, *span, BindingKind::Const),
                    );
                    if let Some(ty) = ty
                        && let Some(explicit_ty) = Self::record_recoverable(
                            errors,
                            self.type_from_mir_type_ref(self.current_owner()?, ty),
                        )
                    {
                        Self::record_recoverable(
                            errors,
                            self.record_decl_local_type(name, *id, *span, explicit_ty.clone()),
                        );
                        Self::record_recoverable(errors, self.record_expr_type(value, explicit_ty));
                    }
                    Self::record_recoverable(errors, self.record_phase(value, Phase::Const));
                    if let Some(kind) = ty.as_ref().and_then(|ty| self.mir_type_kind(ty))
                        && let Some(local_id) = local_id
                    {
                        nested = nested.with_local_binding(
                            local_id,
                            kind,
                            nested.value_for_kind(kind, value, self),
                        );
                    }
                }
                HirStmt::Let {
                    id,
                    name,
                    value,
                    span,
                    ty,
                    ..
                } => {
                    Self::record_recoverable(
                        errors,
                        self.record_decl_local_binding(name, *id, *span, BindingKind::Local),
                    );
                    if let Some(ty) = ty
                        && let Some(explicit_ty) = Self::record_recoverable(
                            errors,
                            self.type_from_mir_type_ref(self.current_owner()?, ty),
                        )
                    {
                        Self::record_recoverable(
                            errors,
                            self.record_decl_local_type(name, *id, *span, explicit_ty),
                        );
                    }
                    if let Some(value) = value {
                        Self::record_recoverable(errors, self.record_phase(value, Phase::Hardware));
                        self.check_let_binding_expr(name, value, &nested, errors)?;
                    }
                }
                HirStmt::Signal {
                    id,
                    name,
                    ty,
                    value,
                    span,
                } => {
                    Self::record_recoverable(
                        errors,
                        self.record_decl_local_binding(name, *id, *span, BindingKind::Local),
                    );
                    if let Some(ty) = ty
                        && let Some(explicit_ty) = Self::record_recoverable(
                            errors,
                            self.type_from_mir_type_ref(self.current_owner()?, ty),
                        )
                    {
                        Self::record_recoverable(
                            errors,
                            self.record_decl_local_type(name, *id, *span, explicit_ty),
                        );
                    }
                    if let Some(value) = value {
                        Self::record_recoverable(errors, self.record_phase(value, Phase::Hardware));
                        self.check_signal_initializer_expr(value, errors)?;
                    }
                }
                HirStmt::Reg {
                    id,
                    name,
                    ty,
                    reset,
                    span,
                } => {
                    Self::record_recoverable(
                        errors,
                        self.record_decl_local_binding(name, *id, *span, BindingKind::Local),
                    );
                    if let Some(ty) = ty
                        && let Some(explicit_ty) = Self::record_recoverable(
                            errors,
                            self.type_from_mir_type_ref(self.current_owner()?, ty),
                        )
                    {
                        Self::record_recoverable(
                            errors,
                            self.record_decl_local_type(name, *id, *span, explicit_ty),
                        );
                    }
                    if let Some(reset) = reset {
                        if let Some(domain) = &reset.domain {
                            self.check_hardware_value_expr(domain, errors)?;
                        }
                        self.check_hardware_value_expr(&reset.value, errors)?;
                    }
                }
                HirStmt::Next { value, .. } => {
                    Self::record_recoverable(errors, self.record_phase(value, Phase::Hardware));
                    self.check_hardware_value_expr(value, errors)?;
                }
                HirStmt::Drive { target, value, .. } => {
                    self.check_hardware_drive_target(target, errors)?;
                    self.check_hardware_value_expr(value, errors)?;
                }
                HirStmt::Expr(expr) => {
                    Self::record_recoverable(errors, self.record_phase(expr, Phase::Hardware));
                    self.check_hardware_stmt_expr(
                        expr,
                        HardwareBlockMode::Control,
                        errors,
                        true,
                    )?;
                }
                _ => {}
            }
        }
        if let Some(tail) = &body.tail {
            self.check_hardware_stmt_expr(tail, HardwareBlockMode::Control, errors, false)?;
        }
        Ok(nested)
    }

    fn require_const_expr_kind(
        &self,
        expr: &HirBodyExpr,
        env: &TirConstEnv,
        kind: TirConstKind,
        errors: &mut Vec<CompileError>,
    ) {
        match kind {
            TirConstKind::Nat => {
                Self::record_recoverable(
                    errors,
                    self.require_const_nat(expr, env, "assignment value"),
                );
            }
            TirConstKind::Bool => {
                Self::record_recoverable(errors, self.require_const_bool(expr, env));
            }
        }
    }
}

fn tir_type_for_const_kind(kind: TirConstKind) -> TirType {
    match kind {
        TirConstKind::Nat => TirType::Nat,
        TirConstKind::Bool => TirType::Bool,
    }
}

fn tir_const_kind_for_type(ty: &TirType) -> Option<TirConstKind> {
    match ty {
        TirType::Nat => Some(TirConstKind::Nat),
        TirType::Bool => Some(TirConstKind::Bool),
        _ => None,
    }
}
