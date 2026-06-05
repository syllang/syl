use super::{
    context::CapabilityContext,
    field_schema::{
        LocalTypeFacts, ViewFieldSchemaRecord, record_let_view_field_schema,
        record_view_field_schema,
    },
    model::{CapabilityScope, EndpointSide, FieldCaps},
    place::{Place, PlaceResolution},
};
use crate::{
    CapabilityError, CompileError, ConstEvalError, HirError,
    binding::ActualFormalBinder,
    hir::resolve::HirResolution,
    hir::{
        HirBlock, HirBodyExpr, HirCallArg, HirCallable, HirCallableItem, HirDefKind, HirDesign,
        HirDriveCapability, HirExprNode, HirPortDirection, HirSignatureParam,
        HirSignatureResultBinding, HirStmt,
    },
    ir::mir::MirTypeRef,
};
use syl_hir::{DefId, LocalId};
use syl_span::Span;

#[derive(Clone, Copy)]
struct CapabilityLocalDecl<'a> {
    name: &'a str,
    id: Option<LocalId>,
    span: Span,
}

struct FormalArgCheck<'a> {
    formal_owner: DefId,
    param: &'a HirSignatureParam,
    actual: &'a HirBodyExpr,
}

#[derive(Clone, Copy)]
struct ReadCheckContext<'a> {
    scope: &'a CapabilityScope,
    facts: &'a LocalTypeFacts,
}

struct TypeCapabilityRecord<'a> {
    decl: CapabilityLocalDecl<'a>,
    ty: &'a MirTypeRef,
    side: EndpointSide,
}

#[non_exhaustive]
pub(crate) struct CapabilityChecker<'a> {
    ctx: &'a dyn CapabilityContext,
}

impl<'a> CapabilityChecker<'a> {
    pub(crate) fn new(hir: &'a HirDesign) -> Self {
        Self { ctx: hir }
    }

    pub(crate) fn check(&self) -> Result<(), CompileError> {
        for (owner, callable) in self.ctx.callables() {
            if let Some(item) = callable.callable_item() {
                self.check_callable(*owner, item)?;
            }
        }
        Ok(())
    }

    fn check_callable(&self, owner: DefId, item: &HirCallableItem) -> Result<(), CompileError> {
        let mut scope = CapabilityScope::new();
        let mut facts = LocalTypeFacts::default();
        for generic in &item.generics {
            scope.insert(
                self.local_id(&generic.name, generic.id, generic.span)?,
                FieldCaps::read_only(),
            );
        }
        for param in &item.params {
            let caps = self.param_caps(owner, param)?;
            let local = self.local_id(&param.name, param.id, param.span)?;
            scope.insert(local, caps);
            record_view_field_schema(
                self.ctx,
                ViewFieldSchemaRecord {
                    owner,
                    facts: &mut facts,
                    local,
                    ty: &param.ty,
                    side: EndpointSide::Local,
                },
            )?;
        }
        if let Some(result) = &item.result {
            let caps = self.result_caps(owner, result)?;
            let local = self.local_id(&result.name, result.id, result.span)?;
            scope.insert(local, caps);
            record_view_field_schema(
                self.ctx,
                ViewFieldSchemaRecord {
                    owner,
                    facts: &mut facts,
                    local,
                    ty: &result.ty,
                    side: EndpointSide::Local,
                },
            )?;
        }
        self.check_block(owner, &item.body, &scope, &facts)
    }

    fn check_block(
        &self,
        owner: DefId,
        body: &HirBlock,
        scope: &CapabilityScope,
        facts: &LocalTypeFacts,
    ) -> Result<(), CompileError> {
        let mut scope = scope.clone();
        let mut facts = facts.clone();
        self.collect_local_signal_drives(owner, body, &mut scope);
        for stmt in &body.stmts {
            match stmt {
                HirStmt::Signal {
                    id,
                    name,
                    value,
                    ty,
                    span,
                } => {
                    let local = self.local_id(name, *id, *span)?;
                    scope.insert(local, FieldCaps::whole());
                    if let Some(value) = value {
                        self.check_read_expr(
                            owner,
                            value,
                            ReadCheckContext {
                                scope: &scope,
                                facts: &facts,
                            },
                        )?;
                    }
                    if let Some(ty) = ty {
                        self.record_type_caps(
                            owner,
                            &mut scope,
                            TypeCapabilityRecord {
                                decl: CapabilityLocalDecl {
                                    name,
                                    id: *id,
                                    span: *span,
                                },
                                ty,
                                side: EndpointSide::LocalSignal,
                            },
                        )?;
                        record_view_field_schema(
                            self.ctx,
                            ViewFieldSchemaRecord {
                                owner,
                                facts: &mut facts,
                                local,
                                ty,
                                side: EndpointSide::LocalSignal,
                            },
                        )?;
                    }
                }
                HirStmt::Reg { id, name, span, .. } => {
                    scope.insert(self.local_id(name, *id, *span)?, FieldCaps::read_only());
                }
                HirStmt::Let {
                    id,
                    name,
                    value: Some(value),
                    span,
                    ..
                } => {
                    let caps = self.let_caps(owner, value)?;
                    let local = self.local_id(name, *id, *span)?;
                    scope.insert(local, caps);
                    record_let_view_field_schema(self.ctx, owner, &mut facts, local, value)?;
                    self.check_read_expr(
                        owner,
                        value,
                        ReadCheckContext {
                            scope: &scope,
                            facts: &facts,
                        },
                    )?;
                }
                HirStmt::Drive { target, value, .. } => {
                    self.require_write(owner, target, &scope, &facts)?;
                    self.check_read_expr(
                        owner,
                        value,
                        ReadCheckContext {
                            scope: &scope,
                            facts: &facts,
                        },
                    )?;
                }
                HirStmt::Next { value, .. } => {
                    self.check_read_expr(
                        owner,
                        value,
                        ReadCheckContext {
                            scope: &scope,
                            facts: &facts,
                        },
                    )?;
                }
                HirStmt::Expr(expr) => self.check_expr_stmt(
                    owner,
                    expr,
                    ReadCheckContext {
                        scope: &scope,
                        facts: &facts,
                    },
                )?,
                HirStmt::ElabIf {
                    then_block,
                    else_block,
                    ..
                } => {
                    let then_scope = scope.clone();
                    let then_facts = facts.clone();
                    self.check_block(owner, then_block, &then_scope, &then_facts)?;
                    if let Some(block) = else_block {
                        let else_scope = scope.clone();
                        let else_facts = facts.clone();
                        self.check_block(owner, block, &else_scope, &else_facts)?;
                    }
                }
                HirStmt::ElabFor {
                    id,
                    name,
                    body,
                    span,
                    ..
                } => {
                    let mut loop_scope = scope.clone();
                    loop_scope.insert(self.local_id(name, *id, *span)?, FieldCaps::read_only());
                    let loop_facts = facts.clone();
                    self.check_block(owner, body, &loop_scope, &loop_facts)?;
                }
                HirStmt::Const { value, .. } => {
                    self.check_read_expr(
                        owner,
                        value,
                        ReadCheckContext {
                            scope: &scope,
                            facts: &facts,
                        },
                    )?;
                }
                HirStmt::Var { .. }
                | HirStmt::Let { value: None, .. }
                | HirStmt::While { .. }
                | HirStmt::Return(_, _)
                | HirStmt::Error { .. } => {}
                _ => {}
            }
        }
        if let Some(tail) = &body.tail {
            self.check_expr_stmt(
                owner,
                tail,
                ReadCheckContext {
                    scope: &scope,
                    facts: &facts,
                },
            )?;
        }
        Ok(())
    }

    fn collect_local_signal_drives(
        &self,
        owner: DefId,
        body: &HirBlock,
        scope: &mut CapabilityScope,
    ) {
        for stmt in &body.stmts {
            match stmt {
                HirStmt::Drive { target, .. } => self.collect_drive(owner, target, scope),
                HirStmt::ElabIf {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.collect_local_signal_drives(owner, then_block, scope);
                    if let Some(block) = else_block {
                        self.collect_local_signal_drives(owner, block, scope);
                    }
                }
                HirStmt::ElabFor { body, .. } => {
                    self.collect_local_signal_drives(owner, body, scope);
                }
                _ => {}
            }
        }
    }

    fn collect_drive(&self, owner: DefId, target: &HirBodyExpr, scope: &mut CapabilityScope) {
        if let PlaceResolution::Place(place) = self.ctx.resolve_place(owner, target) {
            scope.mark_local_drive(&place);
        }
    }

    fn check_expr_stmt(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        self.check_read_expr(owner, expr, context)
    }

    fn check_read_expr(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        match &expr.node {
            HirExprNode::Ident(_) | HirExprNode::Field { .. } | HirExprNode::Index { .. } => {
                match self.ctx.resolve_place(owner, expr) {
                    PlaceResolution::Place(place) => {
                        self.require_read_place(&place, context.scope, context.facts)?
                    }
                    PlaceResolution::UnresolvedName { name, span } => {
                        return Err(CompileError::lowering_at(
                            CapabilityError::UnresolvedName { name },
                            span,
                        ));
                    }
                    PlaceResolution::NotPlace => {}
                }
                self.check_expr_children(owner, expr, context)
            }
            HirExprNode::Select { arms, .. } => {
                for arm in arms {
                    if self.is_default_pattern(&arm.pattern) {
                        self.check_read_expr(owner, &arm.value, context)?;
                        continue;
                    }
                    if matches!(arm.pattern.node, HirExprNode::Bool(_) | HirExprNode::Int(_)) {
                        return Err(CompileError::lowering_at(
                            CapabilityError::SelectGuardRequiresBit,
                            arm.pattern.span(),
                        ));
                    }
                    self.check_read_expr(owner, &arm.pattern, context)?;
                    self.check_read_expr(owner, &arm.value, context)?;
                }
                Ok(())
            }
            _ => self.check_expr_children(owner, expr, context),
        }
    }

    fn check_expr_children(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        match &expr.node {
            HirExprNode::Unary { expr, .. }
            | HirExprNode::Group(expr)
            | HirExprNode::GenericApp { callee: expr, .. } => {
                self.check_read_expr(owner, expr, context)
            }
            HirExprNode::Binary { left, right, .. } => {
                self.check_read_expr(owner, left, context)?;
                self.check_read_expr(owner, right, context)
            }
            HirExprNode::Call { callee, args } | HirExprNode::Place { callee, args, .. } => {
                self.check_call_args(owner, callee, args, context)
            }
            HirExprNode::Aggregate { fields, .. } => {
                for field in fields {
                    self.check_read_expr(owner, &field.value, context)?;
                }
                Ok(())
            }
            HirExprNode::Match { expr, arms } => {
                self.check_read_expr(owner, expr, context)?;
                for arm in arms {
                    self.check_read_expr(owner, &arm.value, context)?;
                }
                Ok(())
            }
            HirExprNode::Field { base, .. } => self.check_projection_base(owner, base, context),
            HirExprNode::Index { base, index } => {
                self.check_projection_base(owner, base, context)?;
                self.check_read_expr(owner, index, context)
            }
            HirExprNode::CompileError { message } => self.check_read_expr(owner, message, context),
            HirExprNode::Ident(_)
            | HirExprNode::Int(_)
            | HirExprNode::Str(_)
            | HirExprNode::Bool(_)
            | HirExprNode::Block(_)
            | HirExprNode::Range { .. } => Ok(()),
            HirExprNode::Select { .. } => Ok(()),
            HirExprNode::Unsupported => Err(CompileError::lowering_at(
                CapabilityError::UnsupportedHardwareValueExpression,
                expr.span(),
            )),
            _ => Ok(()),
        }
    }

    fn check_projection_base(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        match &expr.node {
            HirExprNode::Index { base, index } => {
                self.check_projection_base(owner, base, context)?;
                self.check_read_expr(owner, index, context)
            }
            HirExprNode::Field { base, .. } | HirExprNode::Group(base) => {
                self.check_projection_base(owner, base, context)
            }
            _ if matches!(
                self.ctx.resolve_place(owner, expr),
                PlaceResolution::Place(_)
            ) =>
            {
                Ok(())
            }
            _ => self.check_read_expr(owner, expr, context),
        }
    }

    fn check_call_args(
        &self,
        owner: DefId,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        let Some((callee_def, callee_name, callable)) = self.callable_for_callee(owner, callee)
        else {
            for arg in args {
                self.check_read_expr(owner, &arg.value, context)?;
            }
            return Ok(());
        };
        let params = callable.params();
        let mut binder = ActualFormalBinder::new(params);
        for arg in args {
            let param = binder.resolve(&callee_name, arg.name.as_deref(), arg.span())?;
            self.check_arg_against_formal(
                owner,
                FormalArgCheck {
                    formal_owner: callee_def,
                    param,
                    actual: &arg.value,
                },
                context,
            )?;
        }
        Ok(())
    }

    fn check_arg_against_formal(
        &self,
        owner: DefId,
        check: FormalArgCheck<'_>,
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        if let Some(caps) =
            self.view_caps(check.formal_owner, &check.param.ty, EndpointSide::Local)?
        {
            return self.check_view_arg_caps(owner, check.actual, &caps, context);
        }
        match check.param.direction {
            HirPortDirection::In => self.check_read_expr(owner, check.actual, context),
            HirPortDirection::InOut => {
                self.check_read_expr(owner, check.actual, context)?;
                self.require_write(owner, check.actual, context.scope, context.facts)
            }
            HirPortDirection::Out => {
                self.require_write(owner, check.actual, context.scope, context.facts)
            }
            _ => self.check_read_expr(owner, check.actual, context),
        }
    }

    fn check_view_arg_caps(
        &self,
        owner: DefId,
        actual: &HirBodyExpr,
        caps: &FieldCaps,
        context: ReadCheckContext<'_>,
    ) -> Result<(), CompileError> {
        let endpoint = self.resolve_required_place(owner, actual)?;
        if endpoint.has_field() {
            return Err(CompileError::lowering_at(
                CapabilityError::UnsupportedHardwareValueExpression,
                actual.span(),
            ));
        }
        for field in caps.readable_fields() {
            self.require_read_place(&endpoint.field_place(field), context.scope, context.facts)?;
        }
        for field in caps.drivable_fields() {
            self.require_write_place(&endpoint.field_place(field), context.scope, context.facts)?;
        }
        Ok(())
    }

    fn require_write(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
        scope: &CapabilityScope,
        facts: &LocalTypeFacts,
    ) -> Result<(), CompileError> {
        let place = self.resolve_required_place(owner, expr)?;
        self.require_write_place(&place, scope, facts)
    }

    fn resolve_required_place(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Place, CompileError> {
        match self.ctx.resolve_place(owner, expr) {
            PlaceResolution::Place(place) => Ok(place),
            PlaceResolution::UnresolvedName { name, span } => Err(CompileError::lowering_at(
                CapabilityError::UnresolvedName { name },
                span,
            )),
            PlaceResolution::NotPlace => Err(CompileError::lowering_at(
                CapabilityError::UnsupportedHardwareValueExpression,
                expr.span(),
            )),
        }
    }

    fn require_write_place(
        &self,
        place: &Place,
        scope: &CapabilityScope,
        facts: &LocalTypeFacts,
    ) -> Result<(), CompileError> {
        if !scope.contains(place) {
            return Err(CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier {
                    name: place.root_name().to_string(),
                },
                place.span(),
            ));
        }
        facts.require_known_field(place)?;
        if scope.can_write(place) {
            Ok(())
        } else {
            Err(CompileError::lowering_at(
                CapabilityError::NotDrivable {
                    target: place.display(),
                },
                place.span(),
            ))
        }
    }

    fn require_read_place(
        &self,
        place: &Place,
        scope: &CapabilityScope,
        facts: &LocalTypeFacts,
    ) -> Result<(), CompileError> {
        if !scope.contains(place) {
            return Err(CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier {
                    name: place.root_name().to_string(),
                },
                place.span(),
            ));
        }
        facts.require_known_field(place)?;
        if scope.can_read(place) {
            Ok(())
        } else {
            Err(CompileError::lowering_at(
                CapabilityError::NotReadable {
                    target: place.display(),
                },
                place.span(),
            ))
        }
    }

    fn is_default_pattern(&self, expr: &HirBodyExpr) -> bool {
        matches!(&expr.node, HirExprNode::Ident(name) if name == "default")
    }

    fn param_caps(
        &self,
        owner: DefId,
        param: &HirSignatureParam,
    ) -> Result<FieldCaps, CompileError> {
        if let Some(caps) = self.view_caps(owner, &param.ty, EndpointSide::Local)? {
            return Ok(caps);
        }
        match param.direction {
            HirPortDirection::In => Ok(FieldCaps::read_only()),
            HirPortDirection::InOut => Ok(FieldCaps::read_write()),
            HirPortDirection::Out => Ok(FieldCaps::write_only()),
            _ => Ok(FieldCaps::read_only()),
        }
    }

    fn result_caps(
        &self,
        owner: DefId,
        result: &HirSignatureResultBinding,
    ) -> Result<FieldCaps, CompileError> {
        let fallback = match result.drive {
            HirDriveCapability::ReadOnly => FieldCaps::read_only,
            HirDriveCapability::ReadWrite => FieldCaps::read_write,
            HirDriveCapability::WriteOnly => FieldCaps::write_only,
            _ => FieldCaps::read_only,
        };
        Ok(self
            .view_caps(owner, &result.ty, EndpointSide::Local)?
            .unwrap_or_else(fallback))
    }

    fn let_caps(&self, owner: DefId, value: &HirBodyExpr) -> Result<FieldCaps, CompileError> {
        let Some((callee_def, _, callable)) = self.callable_from_value(owner, value) else {
            return Ok(FieldCaps::read_only());
        };
        let Some(result_ty) = callable.result().map(|result| &result.ty) else {
            return Ok(FieldCaps::read_only());
        };
        Ok(self
            .view_caps(callee_def, result_ty, EndpointSide::Returned)?
            .unwrap_or_else(FieldCaps::read_only))
    }

    fn callable_from_value(
        &self,
        owner: DefId,
        expr: &HirBodyExpr,
    ) -> Option<(DefId, String, &HirCallable)> {
        match &expr.node {
            HirExprNode::Call { callee, .. } | HirExprNode::Place { callee, .. } => {
                self.callable_for_callee(owner, callee)
            }
            _ => None,
        }
    }

    fn callable_for_callee(
        &self,
        owner: DefId,
        callee: &HirBodyExpr,
    ) -> Option<(DefId, String, &HirCallable)> {
        let root = self.callee_root(callee)?;
        let Some(HirResolution::Def(def)) = self.ctx.expr_resolution(owner, root).ok()? else {
            return None;
        };
        let kind = self.ctx.def_kind(def)?;
        if !matches!(kind, HirDefKind::Cell | HirDefKind::ExternCell) {
            return None;
        }
        let name = self.ctx.def_name(def)?.to_string();
        let callable = self.ctx.callable_by_def(def)?;
        Some((def, name, callable))
    }

    fn callee_root<'b>(&self, callee: &'b HirBodyExpr) -> Option<&'b HirBodyExpr> {
        let mut current = callee;
        loop {
            match &current.node {
                HirExprNode::Ident(_) => return Some(current),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }

    fn record_type_caps(
        &self,
        owner: DefId,
        scope: &mut CapabilityScope,
        record: TypeCapabilityRecord<'_>,
    ) -> Result<(), CompileError> {
        if let Some(caps) = self.view_caps(owner, record.ty, record.side)? {
            scope.insert(
                self.local_id(record.decl.name, record.decl.id, record.decl.span)?,
                caps,
            );
        }
        Ok(())
    }

    fn view_caps(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
        side: EndpointSide,
    ) -> Result<Option<FieldCaps>, CompileError> {
        self.ctx.view_caps(owner, ty, side)
    }

    fn local_id(
        &self,
        name: &str,
        id: Option<LocalId>,
        span: Span,
    ) -> Result<LocalId, CompileError> {
        id.ok_or_else(|| {
            CompileError::lowering_at(
                HirError::MissingHirLocal {
                    name: name.to_string(),
                    start: span.start,
                    end: span.end,
                },
                span,
            )
        })
    }
}
