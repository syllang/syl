pub(crate) mod for_emit;
pub(crate) mod request;

pub(crate) use request::{
    AggregateAssignEmit, ConstEmit, ForEmit, IfEmit, LetPlaceEmit, RegEmit, SignalEmit,
};

use crate::{
    CompileError, DriverError, EirError,
    const_eval::ConstValue,
    eir::{EirExpr, EirItem, EirPlace, EirReset, EirSignalActivity},
    mir::MirTypeRef,
    program::{ElabBlock, ElabExpr, ElabExprNode, ElabStmt},
    tir::TirType,
};
use std::collections::BTreeSet;
use syl_sema::tir::TirGenericArg;
use syl_span::Span;

use super::{EirBuilder, Env, connections::InstanceEmitRequest, connections::ViewSignalSpec};

struct BindVarRequest<'a> {
    id: Option<syl_hir::LocalId>,
    name: &'a str,
    ty: Option<&'a MirTypeRef>,
    value: Option<&'a ElabExpr>,
    span: Span,
}

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn emit_body(
        &self,
        body: &ElabBlock,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        self.emit_body_impl(body, env, false)
    }

    fn emit_body_impl(
        &self,
        body: &ElabBlock,
        env: &mut Env,
        compile_error_as_sv: bool,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut items = Vec::new();
        for stmt in &body.stmts {
            match stmt {
                ElabStmt::Next { .. } => {}
                ElabStmt::Const {
                    name,
                    ty,
                    value,
                    span,
                } => {
                    items.extend(self.emit_const(
                        ConstEmit {
                            name,
                            ty: ty.clone(),
                            value,
                            span: *span,
                        },
                        env,
                    )?);
                }
                ElabStmt::Let {
                    name,
                    value: Some(value),
                    ..
                } => {
                    items.extend(self.emit_let(name, value, env)?);
                }
                ElabStmt::Var {
                    id,
                    name,
                    ty,
                    value,
                    span,
                } => {
                    self.bind_var(
                        BindVarRequest {
                            id: *id,
                            name,
                            ty: ty.as_ref(),
                            value: value.as_ref(),
                            span: *span,
                        },
                        env,
                    )?;
                }
                ElabStmt::Assign {
                    target,
                    value,
                    span,
                } => {
                    self.emit_assign(target, value, *span, env)?;
                }
                ElabStmt::Let { span, .. } => {
                    return Err(CompileError::lowering_at(
                        EirError::LocalBindingLoweringUnsupported,
                        *span,
                    ));
                }
                ElabStmt::Signal {
                    name,
                    ty,
                    value,
                    span,
                } => items.extend(self.emit_signal(
                    SignalEmit {
                        name,
                        ty: ty.clone(),
                        value: value.as_ref(),
                        span: *span,
                    },
                    env,
                )?),
                ElabStmt::Reg {
                    name,
                    ty,
                    reset,
                    span,
                } => items.extend(self.emit_reg(
                    RegEmit {
                        name,
                        ty: ty.clone(),
                        reset: reset.as_ref(),
                        span: *span,
                        body,
                    },
                    env,
                )?),
                ElabStmt::Drive {
                    target,
                    value,
                    span,
                } => items.extend(self.emit_drive(target, value, *span, env)?),
                ElabStmt::Expr(expr) => {
                    items.extend(self.emit_expr_stmt(expr, env, compile_error_as_sv)?);
                }
                ElabStmt::ElabIf {
                    cond,
                    then_block,
                    else_block,
                    span,
                } => items.extend(self.emit_if(
                    IfEmit {
                        cond,
                        then_block,
                        else_block: else_block.as_ref(),
                        span: *span,
                    },
                    env,
                )?),
                ElabStmt::ElabFor {
                    name,
                    range,
                    body,
                    span,
                } => items.extend(self.emit_for(
                    ForEmit {
                        name,
                        range_expr: range,
                        body,
                        span: *span,
                    },
                    env,
                )?),
                ElabStmt::While { span } | ElabStmt::Return(span) => {
                    return Err(CompileError::lowering_at(
                        EirError::IllegalHardwareStatement,
                        *span,
                    ));
                }
                ElabStmt::Error { span } => {
                    return Err(CompileError::lowering_at(
                        EirError::InvalidElaborationExpression,
                        *span,
                    ));
                }
            }
        }
        if let Some(tail) = &body.tail {
            items.extend(self.emit_expr_stmt(tail, env, compile_error_as_sv)?);
        }
        Ok(items)
    }

    fn emit_signal(
        &self,
        request: SignalEmit<'_>,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut items = Vec::new();
        let ty = request
            .ty
            .ok_or_else(|| CompileError::lowering_at(EirError::SignalRequiresType, request.span))?;
        let ty = self.subst_type_vars(&ty, &env.type_replacements);
        let physical_name = env.local_name(request.name);
        if let Some(view_items) = self.emit_view_signals(
            ViewSignalSpec {
                binding: request.name,
                physical_prefix: &physical_name,
                ty: &ty,
                span: request.span,
            },
            env,
        ) {
            items.extend(view_items);
            if let Some(value) = request.value {
                if let ElabExprNode::Place {
                    callee,
                    args,
                    inplace,
                } = &value.node
                {
                    items.extend(self.emit_instance(InstanceEmitRequest {
                        inst_name: &physical_name,
                        callee,
                        args,
                        env,
                        inplace: *inplace,
                        span: value.span(),
                    })?);
                } else {
                    items.push(EirItem::Drive {
                        lhs: EirPlace::Ident(physical_name.clone()),
                        rhs: self.elab_expr(value, env),
                        reads: self.elab_read_places(value, env),
                        origin: env.origin(value.span()),
                    });
                }
            }
            env.insert(request.name, EirExpr::ident(physical_name), ty);
            return Ok(items);
        }
        let width = self.width_bound(env.owner, &ty);
        items.push(EirItem::Signal {
            width,
            name: physical_name.clone(),
            activity: EirSignalActivity::Required,
            origin: env.origin(request.span),
        });
        env.insert(request.name, EirExpr::ident(&physical_name), ty);
        if let Some(value) = request.value {
            if let ElabExprNode::Place {
                callee,
                args,
                inplace,
            } = &value.node
            {
                items.extend(self.emit_instance(InstanceEmitRequest {
                    inst_name: &physical_name,
                    callee,
                    args,
                    env,
                    inplace: *inplace,
                    span: value.span(),
                })?);
            } else {
                items.push(EirItem::Drive {
                    lhs: EirPlace::Ident(physical_name.clone()),
                    rhs: self.elab_expr(value, env),
                    reads: self.elab_read_places(value, env),
                    origin: env.origin(value.span()),
                });
            }
        }
        Ok(items)
    }

    fn emit_const(
        &self,
        request: ConstEmit<'_>,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let ty = request
            .ty
            .unwrap_or_else(|| MirTypeRef::path_type(vec!["nat".to_string()], request.span));
        let physical_name = env.local_name(request.name);
        let code = match self.elab_const_value(request.value, env)? {
            ConstValue::Nat(value) => EirExpr::Int(value),
            ConstValue::Bool(value) => EirExpr::Bool(value),
            ConstValue::Unknown(_) => EirExpr::ident(&physical_name),
            _ => EirExpr::unsupported("unsupported const value"),
        };
        env.insert(request.name, code, ty);
        if matches!(
            self.elab_const_value(request.value, env)?,
            ConstValue::Unknown(_)
        ) {
            Ok(vec![EirItem::StaticParam {
                name: physical_name,
                value: self.elab_expr(request.value, env),
                origin: env.origin(request.span),
            }])
        } else {
            Ok(Vec::new())
        }
    }

    fn emit_reg(&self, request: RegEmit<'_>, env: &mut Env) -> Result<Vec<EirItem>, CompileError> {
        let ty = request.ty.ok_or_else(|| {
            CompileError::lowering_at(EirError::RegisterRequiresType, request.span)
        })?;
        let ty = self.subst_type_vars(&ty, &env.type_replacements);
        let physical_name = env.local_name(request.name);
        let width = self.width_bound(env.owner, &ty);
        env.insert(request.name, EirExpr::ident(&physical_name), ty);
        let next = self
            .next_expr(request.name, request.body, env)?
            .unwrap_or_else(|| EirExpr::ident(&physical_name));
        let mut reads = self.next_reads(request.name, request.body, env)?;
        let clock = request
            .reset
            .and_then(|reset| reset.domain.as_ref())
            .and_then(|expr| env.clock_for_elab_reset_expr(expr, self))
            .or_else(|| env.single_by_type("Clock", self))
            .ok_or_else(|| {
                CompileError::lowering_at(EirError::RegisterRequiresClock, request.span)
            })?;
        let reset = if let Some(reset) = request.reset {
            let reset_expr = reset
                .domain
                .as_ref()
                .map(|expr| self.elab_expr(expr, env))
                .or_else(|| env.single_by_type("Reset", self))
                .ok_or_else(|| {
                    CompileError::lowering_at(EirError::RegisterRequiresReset, reset.span)
                })?;
            let reset_value = self.elab_expr(&reset.value, env);
            reads.extend(self.elab_read_places(&reset.value, env));
            Some(EirReset::new(reset_expr, reset_value))
        } else {
            None
        };
        Ok(vec![
            EirItem::Storage {
                width,
                name: physical_name.clone(),
                origin: env.origin(request.span),
            },
            EirItem::ClockedStorage {
                clock,
                target: EirPlace::Ident(physical_name),
                reset: reset.map(Box::new),
                next,
                reads,
                origin: env.origin(request.span),
            },
        ])
    }

    fn next_map<'b>(
        &self,
        body: &'b ElabBlock,
    ) -> Result<std::collections::HashMap<String, (&'b ElabExpr, Span)>, CompileError> {
        let mut nexts = std::collections::HashMap::new();
        for stmt in &body.stmts {
            if let ElabStmt::Next { name, value, span } = stmt
                && nexts.insert(name.clone(), (value, *span)).is_some()
            {
                return Err(CompileError::lowering_at(
                    DriverError::DuplicateNextDriver { name: name.clone() },
                    *span,
                ));
            }
        }
        Ok(nexts)
    }

    fn next_expr(
        &self,
        name: &str,
        body: &ElabBlock,
        env: &Env,
    ) -> Result<Option<EirExpr>, CompileError> {
        let direct = self.next_map(body)?;
        let mut found: Option<(EirExpr, Span)> = None;
        if let Some((expr, span)) = direct.get(name) {
            found = Some((self.elab_expr(expr, env), *span));
        }
        for stmt in &body.stmts {
            if let ElabStmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
                ..
            } = stmt
            {
                let conditional = match self.elab_const_bool(cond, env)? {
                    Some(true) => self.next_expr(name, then_block, env)?,
                    Some(false) => else_block
                        .as_ref()
                        .map(|block| self.next_expr(name, block, env))
                        .transpose()?
                        .flatten(),
                    None => {
                        let then_next = self.next_expr(name, then_block, env)?;
                        let else_next = else_block
                            .as_ref()
                            .map(|block| self.next_expr(name, block, env))
                            .transpose()?
                            .flatten();
                        if then_next.is_some() || else_next.is_some() {
                            let hold = env
                                .vars
                                .get(name)
                                .map(|var| var.code.clone())
                                .unwrap_or_else(|| EirExpr::ident(name));
                            let then_code = then_next.unwrap_or_else(|| hold.clone());
                            let else_code = else_next.unwrap_or(hold);
                            Some(EirExpr::mux(
                                self.elab_expr(cond, env),
                                then_code,
                                else_code,
                            ))
                        } else {
                            None
                        }
                    }
                };
                if let Some(code) = conditional {
                    if found.is_some() {
                        return Err(CompileError::lowering_at(
                            DriverError::DuplicateNextDriver {
                                name: name.to_string(),
                            },
                            *span,
                        ));
                    }
                    found = Some((code, *span));
                }
            }
        }
        Ok(found.map(|(expr, _)| expr))
    }

    fn next_reads(
        &self,
        name: &str,
        body: &ElabBlock,
        env: &Env,
    ) -> Result<Vec<EirExpr>, CompileError> {
        let direct = self.next_map(body)?;
        let mut reads = Vec::new();
        if let Some((expr, _)) = direct.get(name) {
            reads.extend(self.elab_read_places(expr, env));
        }
        for stmt in &body.stmts {
            if let ElabStmt::ElabIf {
                then_block,
                else_block,
                ..
            } = stmt
            {
                reads.extend(self.next_reads(name, then_block, env)?);
                if let Some(block) = else_block {
                    reads.extend(self.next_reads(name, block, env)?);
                }
            }
        }
        Ok(reads)
    }

    fn emit_if(&self, request: IfEmit<'_>, env: &mut Env) -> Result<Vec<EirItem>, CompileError> {
        let resolved_cond = if let Some(value) = Self::local_const_bool(request.cond, env) {
            Some(value)
        } else {
            self.elab_const_bool(request.cond, env)?
        };
        match resolved_cond {
            Some(true) => {
                let mut then_env = env.clone();
                let items = self.emit_body_impl(request.then_block, &mut then_env, false)?;
                self.sync_visible_software_locals(&then_env, env);
                return Ok(items);
            }
            Some(false) => {
                if let Some(block) = request.else_block {
                    let mut else_env = env.clone();
                    let items = self.emit_body_impl(block, &mut else_env, false)?;
                    self.sync_visible_software_locals(&else_env, env);
                    return Ok(items);
                }
                return Ok(Vec::new());
            }
            None => {}
        }
        let mut then_env = env.clone();
        let then_items = self.emit_control_body(request.then_block, &mut then_env)?;
        let (else_items, else_env) = if let Some(block) = request.else_block {
            let mut else_env = env.clone();
            let items = self.emit_control_body(block, &mut else_env)?;
            (items, Some(else_env))
        } else {
            (Vec::new(), None)
        };
        if let Some(else_env) = else_env.as_ref() {
            self.merge_visible_software_locals_between_branches(&then_env, else_env, env);
        } else {
            self.merge_visible_software_locals_after_loop(&then_env, env);
        }
        if then_items.is_empty() && else_items.is_empty() {
            return Ok(Vec::new());
        }
        let label = env.unique_label("gen_if", request.span);
        Ok(vec![EirItem::SymbolicStaticIf {
            cond: self.elab_expr(request.cond, env),
            label,
            then_items,
            else_items,
            origin: env.origin(request.span),
        }])
    }

    fn emit_control_body(
        &self,
        body: &ElabBlock,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut filtered = body.clone();
        filtered
            .stmts
            .retain(|stmt| !matches!(stmt, ElabStmt::Next { .. }));
        self.emit_body_impl(&filtered, env, true)
    }

    fn emit_let(
        &self,
        name: &str,
        value: &ElabExpr,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        match &value.node {
            ElabExprNode::Place {
                callee,
                args,
                inplace,
            } => self.emit_let_place(
                LetPlaceEmit {
                    name,
                    callee,
                    args,
                    inplace: *inplace,
                    value,
                },
                env,
            ),
            ElabExprNode::For {
                name: loop_name,
                range,
                body,
            } => self.emit_for_let(name, loop_name, range, body, value.span(), env),
            _ => Ok(self.bind_let_expr(name, value, env)),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_for_let(
        &self,
        binding_name: &str,
        loop_name: &str,
        range: &ElabExpr,
        body: &ElabBlock,
        span: Span,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let local_name = env.local_name(binding_name);
        env.insert(
            binding_name,
            EirExpr::ident(&local_name),
            MirTypeRef::path_type(vec!["Bit".to_string()], span),
        );
        let mut loop_env = env.clone();
        loop_env.prefix = Some(format!("{local_name}[{loop_name}]"));
        self.emit_for(
            ForEmit {
                name: loop_name,
                range_expr: range,
                body,
                span,
            },
            &mut loop_env,
        )
    }

    fn emit_let_place(
        &self,
        request: LetPlaceEmit<'_>,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let physical_name = env.local_name(request.name);
        let Some(result_ty) = self.callable_result_type_from_elab(request.callee, env) else {
            let items = self.emit_instance(InstanceEmitRequest {
                inst_name: &physical_name,
                callee: request.callee,
                args: request.args,
                env,
                inplace: request.inplace,
                span: request.value.span(),
            })?;
            return Ok(items);
        };
        env.insert(
            request.name,
            EirExpr::ident(&physical_name),
            result_ty.clone(),
        );
        let mut items =
            self.emit_result_signals(&physical_name, &result_ty, request.value.span(), env);
        items.extend(self.emit_instance(InstanceEmitRequest {
            inst_name: &physical_name,
            callee: request.callee,
            args: request.args,
            env,
            inplace: request.inplace,
            span: request.value.span(),
        })?);
        Ok(items)
    }

    fn bind_let_expr(&self, name: &str, value: &ElabExpr, env: &mut Env) -> Vec<EirItem> {
        env.insert(
            name,
            self.elab_expr(value, env),
            MirTypeRef::path_type(vec!["Bit".to_string()], value.span()),
        );
        Vec::new()
    }

    fn bind_var(&self, binding: BindVarRequest<'_>, env: &mut Env) -> Result<(), CompileError> {
        let ty = binding
            .ty
            .cloned()
            .or_else(|| {
                binding
                    .id
                    .and_then(|local| self.program.local_type(local))
                    .and_then(|ty| self.tir_to_mir_type(ty, binding.span))
            })
            .or_else(|| Self::software_local_type_from_env(binding.value, env))
            .or_else(|| {
                binding
                    .value
                    .and_then(|expr| self.infer_software_expr_type(expr, env))
            })
            .or_else(|| {
                binding.value.and_then(|expr| match &expr.node {
                    ElabExprNode::Aggregate { ty, .. } => Some(ty.clone()),
                    _ => None,
                })
            })
            .ok_or_else(|| {
                CompileError::lowering_at(EirError::InvalidElaborationExpression, binding.span)
            })?;
        let code = binding
            .value
            .map(|expr| self.elab_expr(expr, env))
            .unwrap_or_else(|| EirExpr::unsupported("uninitialized mutable local"));
        env.insert_software_local(binding.name, code, ty);
        self.sync_software_local_fields(binding.name, binding.value, env);
        self.rebuild_software_root_binding(binding.name, env);
        Ok(())
    }

    fn emit_assign(
        &self,
        target: &ElabExpr,
        value: &ElabExpr,
        span: Span,
        env: &mut Env,
    ) -> Result<(), CompileError> {
        match &target.node {
            ElabExprNode::Ident(name) => {
                let Some(var) = env.var(name) else {
                    return Err(CompileError::lowering_at(
                        EirError::InvalidElaborationExpression,
                        span,
                    ));
                };
                if !var.software_local {
                    return Err(CompileError::lowering_at(
                        EirError::IllegalHardwareStatement,
                        span,
                    ));
                }
                let ty = var.ty.clone();
                env.insert_software_local(name, self.elab_expr(value, env), ty);
                self.sync_software_local_fields(name, Some(value), env);
                self.rebuild_software_root_binding(name, env);
                Ok(())
            }
            ElabExprNode::Field { base, field } => {
                let Some(root) = Self::field_root_name(base) else {
                    return Err(CompileError::lowering_at(
                        EirError::InvalidElaborationExpression,
                        span,
                    ));
                };
                let Some(var) = env.var(root) else {
                    return Err(CompileError::lowering_at(
                        EirError::InvalidElaborationExpression,
                        span,
                    ));
                };
                if !var.software_local {
                    return Err(CompileError::lowering_at(
                        EirError::IllegalHardwareStatement,
                        span,
                    ));
                }
                let field_name = format!("{root}.{field}");
                let field_ty = env
                    .var(&field_name)
                    .map(|var| var.ty.clone())
                    .or_else(|| self.infer_software_expr_type(target, env))
                    .ok_or_else(|| {
                        CompileError::lowering_at(EirError::InvalidElaborationExpression, span)
                    })?;
                env.insert_software_local(field_name, self.elab_expr(value, env), field_ty);
                self.rebuild_software_root_binding(root, env);
                Ok(())
            }
            _ => Err(CompileError::lowering_at(
                EirError::InvalidElaborationExpression,
                span,
            )),
        }
    }

    fn emit_expr_stmt(
        &self,
        expr: &ElabExpr,
        env: &Env,
        compile_error_as_sv: bool,
    ) -> Result<Vec<EirItem>, CompileError> {
        if let ElabExprNode::CompileError { message } = &expr.node {
            if !compile_error_as_sv {
                return Err(CompileError::lowering_at(
                    EirError::InvalidElaborationExpression,
                    expr.span(),
                ));
            }
            return Ok(vec![EirItem::InitialError {
                message: self.elab_expr(message, env),
                origin: env.origin(expr.span()),
            }]);
        }
        Err(CompileError::lowering_at(
            EirError::InvalidElaborationExpression,
            expr.span(),
        ))
    }

    fn emit_drive(
        &self,
        target: &ElabExpr,
        value: &ElabExpr,
        span: Span,
        env: &Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        match (&target.node, &value.node) {
            (ElabExprNode::Ident(_), ElabExprNode::Aggregate { ty, fields }) => self
                .emit_aggregate_drive(
                    AggregateAssignEmit {
                        target,
                        ty,
                        fields,
                        span,
                    },
                    env,
                ),
            _ => Ok(vec![EirItem::Drive {
                lhs: self.place_expr(target, env)?,
                rhs: self.elab_expr(value, env),
                reads: self.elab_read_places(value, env),
                origin: env.origin(span),
            }]),
        }
    }

    fn emit_aggregate_drive(
        &self,
        request: AggregateAssignEmit<'_>,
        env: &Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut reads = Vec::new();
        for field in request.fields {
            reads.extend(self.elab_read_places(&field.value, env));
        }
        reads.sort_by_key(EirExpr::fact_key);
        reads.dedup_by_key(|expr| expr.fact_key());
        Ok(vec![EirItem::Drive {
            lhs: self.place_expr(request.target, env)?,
            rhs: self.elab_aggregate_expr(request.ty, request.fields, env),
            reads,
            origin: env.origin(request.span),
        }])
    }

    fn place_expr(&self, expr: &ElabExpr, env: &Env) -> Result<EirPlace, CompileError> {
        let lowered = self.elab_expr(expr, env);
        EirPlace::try_from(&lowered).map_err(|_| {
            CompileError::lowering_at(EirError::UnsupportedHardwareValueExpression, expr.span())
        })
    }

    fn sync_visible_software_locals(&self, source: &Env, target: &mut Env) {
        let visible = target.vars.keys().cloned().collect::<Vec<_>>();
        for (name, var) in &source.vars {
            let root_visible = name
                .split_once('.')
                .and_then(|(root, _)| target.var(root))
                .is_some_and(|var| var.software_local);
            if (visible.iter().any(|existing| existing == name) || root_visible)
                && var.software_local
            {
                target.insert_software_local(name.clone(), var.code.clone(), var.ty.clone());
            }
        }
    }

    fn merge_visible_software_locals_between_branches(
        &self,
        then_env: &Env,
        else_env: &Env,
        target: &mut Env,
    ) {
        for name in self.visible_software_local_names(target, &[then_env, else_env]) {
            let Some(current) = target.var(&name) else {
                continue;
            };
            let then_var = then_env.var(&name);
            let else_var = else_env.var(&name);
            let merged = match (then_var, else_var) {
                (Some(then_var), Some(else_var))
                    if then_var.ty == else_var.ty && then_var.code == else_var.code =>
                {
                    (then_var.code.clone(), then_var.ty.clone())
                }
                _ => (self.unknown_software_local_expr(&name), current.ty.clone()),
            };
            target.insert_software_local(name, merged.0, merged.1);
        }
    }

    fn merge_visible_software_locals_after_loop(&self, source: &Env, target: &mut Env) {
        for name in self.visible_software_local_names(target, &[source]) {
            let Some(current) = target.var(&name) else {
                continue;
            };
            let merged = match source.var(&name) {
                Some(updated) if updated.ty == current.ty && updated.code == current.code => {
                    (updated.code.clone(), updated.ty.clone())
                }
                Some(_) => (self.unknown_software_local_expr(&name), current.ty.clone()),
                None => (current.code.clone(), current.ty.clone()),
            };
            target.insert_software_local(name, merged.0, merged.1);
        }
    }

    fn visible_software_local_names(&self, base: &Env, sources: &[&Env]) -> Vec<String> {
        let mut names = BTreeSet::new();
        for (name, var) in &base.vars {
            if var.software_local {
                names.insert(name.clone());
            }
        }
        for source in sources {
            for (name, var) in &source.vars {
                if !var.software_local {
                    continue;
                }
                let Some((root, _)) = name.split_once('.') else {
                    continue;
                };
                if base.var(root).is_some_and(|root| root.software_local) {
                    names.insert(name.clone());
                }
            }
        }
        names.into_iter().collect()
    }

    fn unknown_software_local_expr(&self, name: &str) -> EirExpr {
        EirExpr::ident(format!("__unknown_{}", name.replace('.', "_")))
    }

    fn sync_software_local_fields(&self, name: &str, value: Option<&ElabExpr>, env: &mut Env) {
        let Some(value) = value else {
            return;
        };
        match &value.node {
            ElabExprNode::Aggregate { fields, .. } => {
                for field in fields {
                    if let Some(field_ty) = self
                        .infer_software_expr_type(&field.value, env)
                        .or_else(|| Self::syntax_software_expr_type(&field.value))
                    {
                        env.insert_software_local(
                            format!("{name}.{}", field.name),
                            self.elab_expr(&field.value, env),
                            field_ty,
                        );
                    }
                }
            }
            ElabExprNode::Ident(source) => self.copy_software_local_fields(source, name, env),
            ElabExprNode::Group(inner) => self.sync_software_local_fields(name, Some(inner), env),
            _ => self.unknown_software_local_fields(name, env),
        }
    }

    fn copy_software_local_fields(&self, source: &str, target: &str, env: &mut Env) {
        let prefix = format!("{source}.");
        let fields = env
            .vars
            .iter()
            .filter(|(name, var)| var.software_local && name.starts_with(&prefix))
            .map(|(name, var)| {
                (
                    format!("{target}.{}", &name[prefix.len()..]),
                    var.code.clone(),
                    var.ty.clone(),
                )
            })
            .collect::<Vec<_>>();
        for (name, code, ty) in fields {
            env.insert_software_local(name, code, ty);
        }
    }

    fn rebuild_software_root_binding(&self, root: &str, env: &mut Env) {
        let Some(root_var) = env.var(root).cloned() else {
            return;
        };
        if !root_var.software_local {
            return;
        }
        let prefix = format!("{root}.");
        let mut field_codes = env
            .vars
            .iter()
            .filter(|(name, var)| var.software_local && name.starts_with(&prefix))
            .map(|(name, var)| (name.clone(), var.code.clone()))
            .collect::<Vec<_>>();
        if field_codes.is_empty() {
            return;
        }
        field_codes.sort_by(|lhs, rhs| lhs.0.cmp(&rhs.0));
        let code = EirExpr::Concat(field_codes.into_iter().map(|(_, code)| code).collect());
        env.insert_software_local(root, code, root_var.ty);
    }

    fn unknown_software_local_fields(&self, root: &str, env: &mut Env) {
        let prefix = format!("{root}.");
        let fields = env
            .vars
            .iter()
            .filter(|(name, var)| var.software_local && name.starts_with(&prefix))
            .map(|(name, var)| (name.clone(), var.ty.clone()))
            .collect::<Vec<_>>();
        for (name, ty) in fields {
            env.insert_software_local(name.clone(), self.unknown_software_local_expr(&name), ty);
        }
    }

    fn local_const_bool(expr: &ElabExpr, env: &Env) -> Option<bool> {
        match &expr.node {
            ElabExprNode::Bool(value) => Some(*value),
            ElabExprNode::Ident(name) => {
                let expr = &env.var(name)?.code;
                Self::eval_local_bool_expr(expr, env)
            }
            ElabExprNode::Field { base, field } => Self::field_binding_expr(base, field, env)
                .and_then(|expr| Self::eval_local_bool_expr(expr, env)),
            ElabExprNode::Group(inner) => Self::local_const_bool(inner, env),
            ElabExprNode::Unary {
                op: crate::mir::MirUnaryOp::Not | crate::mir::MirUnaryOp::NotWord,
                expr,
            } => Self::local_const_bool(expr, env).map(|value| !value),
            ElabExprNode::Binary { op, left, right } => match op {
                crate::mir::MirBinaryOp::AndAnd => {
                    Some(Self::local_const_bool(left, env)? && Self::local_const_bool(right, env)?)
                }
                crate::mir::MirBinaryOp::OrOr => {
                    Some(Self::local_const_bool(left, env)? || Self::local_const_bool(right, env)?)
                }
                crate::mir::MirBinaryOp::Eq => {
                    Some(Self::local_const_nat(left, env)? == Self::local_const_nat(right, env)?)
                }
                crate::mir::MirBinaryOp::NotEq => {
                    Some(Self::local_const_nat(left, env)? != Self::local_const_nat(right, env)?)
                }
                crate::mir::MirBinaryOp::Lt => {
                    Some(Self::local_const_nat(left, env)? < Self::local_const_nat(right, env)?)
                }
                crate::mir::MirBinaryOp::LtEq => {
                    Some(Self::local_const_nat(left, env)? <= Self::local_const_nat(right, env)?)
                }
                crate::mir::MirBinaryOp::Gt => {
                    Some(Self::local_const_nat(left, env)? > Self::local_const_nat(right, env)?)
                }
                crate::mir::MirBinaryOp::GtEq => {
                    Some(Self::local_const_nat(left, env)? >= Self::local_const_nat(right, env)?)
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn local_const_nat(expr: &ElabExpr, env: &Env) -> Option<u64> {
        match &expr.node {
            ElabExprNode::Int(value) => Some(*value),
            ElabExprNode::Ident(name) => {
                let expr = &env.var(name)?.code;
                Self::eval_local_nat_expr(expr, env)
            }
            ElabExprNode::Field { base, field } => Self::field_binding_expr(base, field, env)
                .and_then(|expr| Self::eval_local_nat_expr(expr, env)),
            ElabExprNode::Group(inner) => Self::local_const_nat(inner, env),
            ElabExprNode::Binary { op, left, right } => {
                let lhs = Self::local_const_nat(left, env)?;
                let rhs = Self::local_const_nat(right, env)?;
                match op {
                    crate::mir::MirBinaryOp::Add => Some(lhs + rhs),
                    crate::mir::MirBinaryOp::Sub => Some(lhs.saturating_sub(rhs)),
                    crate::mir::MirBinaryOp::Mul => Some(lhs * rhs),
                    crate::mir::MirBinaryOp::Div if rhs != 0 => Some(lhs / rhs),
                    crate::mir::MirBinaryOp::Rem if rhs != 0 => Some(lhs % rhs),
                    crate::mir::MirBinaryOp::Shl => Some(lhs << rhs),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn field_binding_expr<'b>(base: &ElabExpr, field: &str, env: &'b Env) -> Option<&'b EirExpr> {
        env.var(&format!("{}.{}", Self::field_root_name(base)?, field))
            .map(|var| &var.code)
    }

    fn eval_local_bool_expr(expr: &EirExpr, env: &Env) -> Option<bool> {
        match expr {
            EirExpr::Bool(value) => Some(*value),
            EirExpr::Ident(name) => env.var(name).and_then(|var| {
                if matches!(&var.code, EirExpr::Ident(inner) if inner == name) {
                    None
                } else {
                    Self::eval_local_bool_expr(&var.code, env)
                }
            }),
            EirExpr::Unary {
                op: crate::eir::EirUnaryOp::Not,
                expr,
            } => Self::eval_local_bool_expr(expr, env).map(|value| !value),
            EirExpr::Binary { op, left, right } => match op {
                crate::eir::EirBinaryOp::AndAnd => Some(
                    Self::eval_local_bool_expr(left, env)?
                        && Self::eval_local_bool_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::OrOr => Some(
                    Self::eval_local_bool_expr(left, env)?
                        || Self::eval_local_bool_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::Eq => Some(
                    Self::eval_local_nat_expr(left, env)? == Self::eval_local_nat_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::NotEq => Some(
                    Self::eval_local_nat_expr(left, env)? != Self::eval_local_nat_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::Lt => Some(
                    Self::eval_local_nat_expr(left, env)? < Self::eval_local_nat_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::LtEq => Some(
                    Self::eval_local_nat_expr(left, env)? <= Self::eval_local_nat_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::Gt => Some(
                    Self::eval_local_nat_expr(left, env)? > Self::eval_local_nat_expr(right, env)?,
                ),
                crate::eir::EirBinaryOp::GtEq => Some(
                    Self::eval_local_nat_expr(left, env)? >= Self::eval_local_nat_expr(right, env)?,
                ),
                _ => None,
            },
            _ => None,
        }
    }

    fn eval_local_nat_expr(expr: &EirExpr, env: &Env) -> Option<u64> {
        match expr {
            EirExpr::Int(value) => Some(*value),
            EirExpr::Ident(name) => env.var(name).and_then(|var| {
                if matches!(&var.code, EirExpr::Ident(inner) if inner == name) {
                    None
                } else {
                    Self::eval_local_nat_expr(&var.code, env)
                }
            }),
            EirExpr::Binary { op, left, right } => {
                let lhs = Self::eval_local_nat_expr(left, env)?;
                let rhs = Self::eval_local_nat_expr(right, env)?;
                match op {
                    crate::eir::EirBinaryOp::Add => Some(lhs + rhs),
                    crate::eir::EirBinaryOp::Sub => Some(lhs.saturating_sub(rhs)),
                    crate::eir::EirBinaryOp::Mul => Some(lhs * rhs),
                    crate::eir::EirBinaryOp::Div if rhs != 0 => Some(lhs / rhs),
                    crate::eir::EirBinaryOp::Rem if rhs != 0 => Some(lhs % rhs),
                    crate::eir::EirBinaryOp::Shl => Some(lhs << rhs),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    fn field_root_name(expr: &ElabExpr) -> Option<&str> {
        match &expr.node {
            ElabExprNode::Ident(name) => Some(name),
            ElabExprNode::Field { base, .. } | ElabExprNode::Group(base) => {
                Self::field_root_name(base)
            }
            _ => None,
        }
    }

    fn software_local_type_from_env(expr: Option<&ElabExpr>, env: &Env) -> Option<MirTypeRef> {
        let expr = expr?;
        match &expr.node {
            ElabExprNode::Ident(name) => env.var(name).map(|var| var.ty.clone()),
            ElabExprNode::Group(inner) => Self::software_local_type_from_env(Some(inner), env),
            _ => None,
        }
    }

    fn infer_software_expr_type(&self, expr: &ElabExpr, env: &Env) -> Option<MirTypeRef> {
        let owner = env.owner?;
        self.program
            .expr_type(owner, expr)
            .and_then(|ty| self.tir_to_mir_type(ty, expr.span()))
    }

    fn syntax_software_expr_type(expr: &ElabExpr) -> Option<MirTypeRef> {
        match &expr.node {
            ElabExprNode::Int(_) => {
                Some(MirTypeRef::path_type(vec!["nat".to_string()], expr.span()))
            }
            ElabExprNode::Bool(_) => {
                Some(MirTypeRef::path_type(vec!["bool".to_string()], expr.span()))
            }
            ElabExprNode::Str(_) => Some(MirTypeRef::path_type(
                vec!["string".to_string()],
                expr.span(),
            )),
            ElabExprNode::Aggregate { ty, .. } => Some(ty.clone()),
            ElabExprNode::Group(inner) => Self::syntax_software_expr_type(inner),
            _ => None,
        }
    }

    fn tir_to_mir_type(&self, ty: &TirType, span: Span) -> Option<MirTypeRef> {
        match ty {
            TirType::Nat => Some(MirTypeRef::path_type(vec!["nat".to_string()], span)),
            TirType::Bool => Some(MirTypeRef::path_type(vec!["bool".to_string()], span)),
            TirType::Bit => Some(MirTypeRef::path_type(vec!["Bit".to_string()], span)),
            TirType::Str => Some(MirTypeRef::path_type(vec!["string".to_string()], span)),
            TirType::Named {
                name, def, args, ..
            } => {
                let base = def
                    .and_then(|def| self.program.canonical_path(def))
                    .map(|path| MirTypeRef::path_type(path.segments().to_vec(), span))
                    .unwrap_or_else(|| MirTypeRef::path_type(vec![name.clone()], span));
                if args.is_empty() {
                    Some(base)
                } else {
                    let args = args
                        .iter()
                        .map(|arg| match arg {
                            TirGenericArg::Type(ty) => self.tir_to_mir_type(ty, span),
                            TirGenericArg::Const(_) => None,
                            _ => None,
                        })
                        .collect::<Option<Vec<_>>>()?;
                    Some(MirTypeRef::generic_type(base, args, span))
                }
            }
            _ => None,
        }
    }
}
