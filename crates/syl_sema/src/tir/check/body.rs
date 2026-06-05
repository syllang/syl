mod elab;

use super::super::consts::{TirConstEnv, TirConstKind};
use super::super::{
    BindingKind, BuiltinIntrinsic, BuiltinResolver, HardwareBlockMode, Phase, TirType,
    TypePhaseChecker,
};
use crate::{
    CompileError, EirError,
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{HirBlock, HirBodyExpr, HirExprNode, HirLocalKind, HirStmt},
};
use elab::{ElabAssignmentStmt, ElabForLoop, ElabVarStmt, HardwareCheckContext};
use syl_hir::LocalId;
use syl_span::Span;

struct PlaceCollectionCheck<'a> {
    id: Option<LocalId>,
    binding_name: &'a str,
    range: &'a HirBodyExpr,
    body: &'a HirBlock,
    span: Span,
    env: &'a TirConstEnv,
}

impl TypePhaseChecker {
    pub(super) fn check_hardware_block(
        &mut self,
        body: &HirBlock,
        env: &TirConstEnv,
        mode: HardwareBlockMode,
        errors: &mut Vec<CompileError>,
    ) -> Result<TirConstEnv, CompileError> {
        let mut env = env.clone();
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
                    env = self.check_elab_var_stmt(
                        ElabVarStmt {
                            id: *id,
                            name,
                            ty: ty.as_ref(),
                            value: value.as_ref(),
                            span: *span,
                        },
                        &env,
                        errors,
                    )?;
                }
                HirStmt::ElabIf {
                    cond,
                    then_block,
                    else_block,
                    ..
                } => {
                    env = self.check_elab_if(
                        cond,
                        then_block,
                        else_block.as_ref(),
                        HardwareCheckContext {
                            env: &env,
                            mode,
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
                    env = self.check_elab_for(
                        ElabForLoop {
                            id: *id,
                            name,
                            span: *span,
                        },
                        range,
                        body,
                        HardwareCheckContext {
                            env: &env,
                            mode,
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
                        env = env.with_local_binding(
                            local_id,
                            kind,
                            env.value_for_kind(kind, value, self),
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
                        self.check_let_binding_expr(name, value, &env, errors)?;
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
                        &env,
                        errors,
                    )? {
                        env = updated_env;
                    } else {
                        errors.push(CompileError::lowering_at(
                            EirError::IllegalHardwareStatement,
                            *span,
                        ));
                    }
                }
                HirStmt::Expr(expr) => {
                    Self::record_recoverable(errors, self.record_phase(expr, Phase::Hardware));
                    if Self::is_runtime_error_stmt_expr(expr) {
                        continue;
                    }
                    self.check_hardware_stmt_expr(expr, mode, errors, true)?;
                }
                _ => {}
            }
        }
        if let Some(tail) = &body.tail {
            self.check_hardware_stmt_expr(tail, mode, errors, false)?;
        }
        Ok(env)
    }

    fn check_let_binding_expr(
        &mut self,
        _binding_name: &str,
        expr: &HirBodyExpr,
        env: &TirConstEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        match &expr.node {
            HirExprNode::Place { callee, args, .. } => {
                self.check_hardware_place_expr(callee, args, errors)
            }
            HirExprNode::For {
                id,
                name,
                range,
                body,
                ..
            } => self.check_place_collection_expr(
                PlaceCollectionCheck {
                    id: *id,
                    binding_name: name,
                    range,
                    body,
                    span: expr.span(),
                    env,
                },
                errors,
            ),
            _ => self.check_hardware_value_expr(expr, errors),
        }
    }

    fn check_place_collection_expr(
        &mut self,
        request: PlaceCollectionCheck<'_>,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        Self::record_recoverable(errors, self.record_phase(request.range, Phase::Const));
        let loop_id = Self::record_recoverable(
            errors,
            self.record_decl_local_binding(
                request.binding_name,
                request.id,
                request.span,
                BindingKind::Const,
            ),
        );
        let loop_env = if let Some(loop_id) = loop_id {
            request
                .env
                .with_local_binding(loop_id, TirConstKind::Nat, None)
        } else {
            request.env.clone()
        };
        let mut body = request.body.clone();
        let tail = body.tail.take();
        self.check_hardware_block(&body, &loop_env, HardwareBlockMode::Normal, errors)?;
        match tail.as_deref().map(|expr| &expr.node) {
            Some(HirExprNode::Place { callee, args, .. }) => {
                self.check_hardware_place_expr(callee, args, errors)?;
            }
            Some(_) | None => {
                errors.push(CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    request.span,
                ));
            }
        }
        Ok(())
    }

    fn check_signal_initializer_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        match &expr.node {
            HirExprNode::Place { callee, args, .. } => {
                self.check_hardware_place_expr(callee, args, errors)
            }
            _ => self.check_hardware_value_expr(expr, errors),
        }
    }

    fn check_hardware_stmt_expr(
        &mut self,
        expr: &HirBodyExpr,
        mode: HardwareBlockMode,
        errors: &mut Vec<CompileError>,
        allow_stmt_builtins: bool,
    ) -> Result<(), CompileError> {
        if matches!(&expr.node, HirExprNode::CompileError { .. })
            && mode == HardwareBlockMode::Control
        {
            return Ok(());
        }
        if let HirExprNode::Place { callee, args, .. } = &expr.node {
            return self.check_hardware_place_expr(callee, args, errors);
        }
        if self.is_runtime_error_builtin_call(expr) {
            if !allow_stmt_builtins {
                errors.push(CompileError::lowering_at(
                    EirError::RuntimeErrorStatementOnly,
                    expr.span(),
                ));
                return Ok(());
            }
            return self.check_runtime_error_stmt_expr(expr, errors);
        }
        if self.is_assert_builtin_call(expr) {
            if !allow_stmt_builtins {
                errors.push(CompileError::lowering_at(
                    EirError::AssertionStatementOnly,
                    expr.span(),
                ));
                return Ok(());
            }
            return self.check_assert_stmt_expr(expr, errors);
        }
        self.check_hardware_value_expr(expr, errors)
    }

    fn is_runtime_error_stmt_expr(expr: &HirBodyExpr) -> bool {
        matches!(&expr.node, HirExprNode::CompileError { .. })
    }

    fn is_runtime_error_builtin_call(&self, expr: &HirBodyExpr) -> bool {
        let HirExprNode::Call { callee, .. } = &expr.node else {
            return false;
        };
        matches!(
            BuiltinResolver::new(&self.hir, self.current_owner).resolve_call_callee(callee),
            Some(BuiltinIntrinsic::Error)
        )
    }

    fn is_assert_builtin_call(&self, expr: &HirBodyExpr) -> bool {
        let HirExprNode::Call { callee, .. } = &expr.node else {
            return false;
        };
        matches!(
            BuiltinResolver::new(&self.hir, self.current_owner).resolve_call_callee(callee),
            Some(BuiltinIntrinsic::Assert)
        )
    }

    fn check_runtime_error_stmt_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let HirExprNode::Call { args, .. } = &expr.node else {
            return Ok(());
        };
        if args.len() != 1 || args[0].name.is_some() {
            errors.push(CompileError::lowering_at(
                EirError::RuntimeErrorRequiresSingleMessage,
                expr.span(),
            ));
            for arg in args {
                self.check_hardware_value_expr(&arg.value, errors)?;
            }
            return Ok(());
        }
        self.check_hardware_value_expr(&args[0].value, errors)
    }

    fn check_assert_stmt_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let HirExprNode::Call { args, .. } = &expr.node else {
            return Ok(());
        };
        if args.len() != 1 || args[0].name.is_some() {
            errors.push(CompileError::lowering_at(
                EirError::AssertionRequiresSingleCondition,
                expr.span(),
            ));
            for arg in args {
                self.check_hardware_value_expr(&arg.value, errors)?;
            }
            return Ok(());
        }
        let condition = &args[0].value;
        self.check_hardware_value_expr(condition, errors)?;
        let owner = self.current_owner()?;
        if !matches!(self.infer_expr_type(owner, condition), TirType::Bit) {
            errors.push(CompileError::lowering_at(
                EirError::AssertionConditionMustBeBit,
                condition.span(),
            ));
        }
        Ok(())
    }
    fn check_hardware_drive_target(
        &mut self,
        target: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.check_place_expr(target, errors)?;
        if let Some(name) = self.reg_drive_root_name(target) {
            errors.push(CompileError::lowering_at(
                EirError::ContinuousDriveTargetIsReg { name },
                target.span(),
            ));
        }
        Ok(())
    }

    fn reg_drive_root_name(&self, expr: &HirBodyExpr) -> Option<String> {
        let owner = self.current_owner?;
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::Ident(name) => {
                    let Ok(Some(HirResolution::Local(id))) =
                        self.hir.expr_resolution(owner, current)
                    else {
                        return None;
                    };
                    let local = self.hir.locals.get(id.get())?;
                    return matches!(local.kind, HirLocalKind::Reg).then(|| name.clone());
                }
                HirExprNode::Field { base, .. } | HirExprNode::Index { base, .. } => {
                    current = base;
                }
                HirExprNode::Group(base) => current = base,
                _ => return None,
            }
        }
    }
}
