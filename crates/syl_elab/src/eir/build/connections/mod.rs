pub(crate) mod request;

pub(crate) use request::{
    CellArgBindingRequest, CellInlineRequest, ConnectionPushRequest, InstanceEmitRequest, PortSpec,
    ResultConnectionRequest, ViewArgConnectionRequest, ViewPortSpec, ViewSignalSpec,
};

use crate::{
    CompileError, EirError,
    eir::{
        EirBinaryOp, EirBound, EirConnection, EirDirection, EirExpr, EirInstance, EirItem, EirPort,
        EirSignalActivity,
    },
    mir::{MirTypeRef, MirTypeRefExt},
    program::{
        ElabBlock, ElabCallArg, ElabCallable, ElabCallableItem, ElabExpr, ElabExprNode,
        ElabPortDirection, ElabStmt, ElabViewDirection,
    },
};
use std::collections::{BTreeSet, HashMap};
use syl_hir::DefId;
use syl_sema::binding::ActualFormalBinder;
use syl_span::Span;

use super::{EirBuilder, Env};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn add_port(
        &self,
        ports: &mut Vec<EirPort>,
        env: &mut Env,
        spec: PortSpec<'_>,
    ) -> Result<(), CompileError> {
        if let Some((base, view, array_len)) = spec.ty.view_shape() {
            self.add_view_ports(
                ports,
                env,
                ViewPortSpec {
                    doc: spec.doc,
                    name: spec.name,
                    base,
                    view,
                    array_len,
                    span: spec.span,
                },
            )?;
            env.insert(spec.name, EirExpr::ident(spec.name), spec.ty.clone());
            return Ok(());
        }
        let direction = match spec.dir {
            ElabPortDirection::In => EirDirection::In,
            ElabPortDirection::InOut => EirDirection::InOut,
            ElabPortDirection::Out => EirDirection::Out,
            ElabPortDirection::Unsupported => {
                return Err(CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    spec.span,
                ));
            }
        };
        let width = self.width_bound(env.owner, spec.ty);
        ports.push(
            EirPort::new(direction, width, spec.name, env.origin(spec.span))
                .with_doc(spec.doc.map(ToOwned::to_owned)),
        );
        env.insert(spec.name, EirExpr::ident(spec.name), spec.ty.clone());
        Ok(())
    }

    fn add_view_ports(
        &self,
        ports: &mut Vec<EirPort>,
        env: &mut Env,
        spec: ViewPortSpec<'_>,
    ) -> Result<(), CompileError> {
        let interface = self
            .interface_for_type(env.owner, spec.base)
            .ok_or_else(|| {
                CompileError::lowering_at(
                    EirError::UnknownInterface {
                        name: self
                            .type_name(spec.base)
                            .unwrap_or_else(|| "<unknown>".to_string()),
                    },
                    spec.base.span(),
                )
            })?;
        let view_decl = interface
            .views
            .iter()
            .find(|decl| decl.name == *spec.view)
            .ok_or_else(|| {
                CompileError::lowering_at(
                    EirError::UnknownView {
                        name: spec.view.to_string(),
                    },
                    spec.span,
                )
            })?;
        for field in &view_decl.fields {
            let field_ty = interface
                .fields
                .iter()
                .find(|decl| decl.name == field.name)
                .map(|decl| self.subst_interface_field_type(env.owner, spec.base, &decl.ty));
            if let Some(field_ty) = field_ty {
                let direction = match field.direction {
                    ElabViewDirection::In => EirDirection::In,
                    ElabViewDirection::InOut => EirDirection::InOut,
                    ElabViewDirection::Out => EirDirection::Out,
                    ElabViewDirection::Unsupported => {
                        return Err(CompileError::lowering_at(
                            EirError::UnsupportedHardwareValueExpression,
                            spec.span,
                        ));
                    }
                };
                let field_width = self.width_bound(env.owner, &field_ty);
                let width = spec
                    .array_len
                    .map(|len| {
                        EirBound::new(
                            format!("({})*({})", self.array_len_key(len), field_width.source()),
                            EirExpr::binary(
                                EirBinaryOp::Mul,
                                self.array_len_expr(len),
                                field_width.expr().clone(),
                            ),
                        )
                    })
                    .unwrap_or(field_width);
                let port_name = format!("{}_{}", spec.name, field.name);
                let env_ty = if let Some(len) = spec.array_len {
                    field_ty.with_array_len(len.clone(), spec.span)
                } else {
                    field_ty
                };
                env.insert(
                    format!("{}.{}", spec.name, field.name),
                    EirExpr::ident(&port_name),
                    env_ty.clone(),
                );
                env.insert(&port_name, EirExpr::ident(&port_name), env_ty);
                ports.push(
                    EirPort::new(direction, width, port_name, env.origin(spec.span))
                        .with_doc(spec.doc.map(ToOwned::to_owned)),
                );
            }
        }
        Ok(())
    }

    pub(super) fn subst_interface_field_type(
        &self,
        owner: Option<syl_hir::DefId>,
        interface_ty: &MirTypeRef,
        field_ty: &MirTypeRef,
    ) -> MirTypeRef {
        let Some(args) = self.type_args(interface_ty) else {
            return field_ty.clone();
        };
        let Some(interface) = self.interface_for_type(owner, interface_ty) else {
            return field_ty.clone();
        };
        let mut replacements: HashMap<String, MirTypeRef> = HashMap::new();
        for (idx, generic) in interface.generics.iter().enumerate() {
            if let Some(arg) = args.get(idx) {
                replacements.insert(generic.name.clone(), arg.clone());
            }
        }
        self.subst_type_vars(field_ty, &replacements)
    }

    pub(super) fn emit_result_signals(
        &self,
        prefix: &str,
        ty: &MirTypeRef,
        span: Span,
        env: &mut Env,
    ) -> Vec<EirItem> {
        self.emit_view_signals(
            ViewSignalSpec {
                binding: prefix,
                physical_prefix: prefix,
                ty,
                span,
            },
            env,
        )
        .unwrap_or_else(|| {
            vec![EirItem::Signal {
                width: self.width_bound(env.owner, ty),
                name: prefix.to_string(),
                activity: EirSignalActivity::Optional,
                origin: env.origin(span),
            }]
        })
    }

    pub(super) fn emit_view_signals(
        &self,
        spec: ViewSignalSpec<'_>,
        env: &mut Env,
    ) -> Option<Vec<EirItem>> {
        let mut items = Vec::new();
        if let Some((base, view, _)) = spec.ty.view_shape()
            && let Some(interface) = self.interface_for_type(env.owner, base)
            && let Some(view_decl) = interface.views.iter().find(|decl| decl.name == *view)
        {
            for field in &view_decl.fields {
                if let Some(field_ty) = self.view_field_type(env.owner, spec.ty, &field.name) {
                    let signal_name = format!("{}_{}", spec.physical_prefix, field.name);
                    let width = self.width_bound(env.owner, &field_ty);
                    items.push(EirItem::Signal {
                        width,
                        name: signal_name.clone(),
                        activity: match field.direction {
                            ElabViewDirection::Out => EirSignalActivity::Required,
                            ElabViewDirection::In | ElabViewDirection::InOut => {
                                EirSignalActivity::Optional
                            }
                            ElabViewDirection::Unsupported => EirSignalActivity::Optional,
                        },
                        origin: env.origin(spec.span),
                    });
                    env.insert(&signal_name, EirExpr::ident(&signal_name), field_ty.clone());
                    env.insert(
                        format!("{}.{}", spec.binding, field.name),
                        EirExpr::ident(signal_name),
                        field_ty,
                    );
                }
            }
            return Some(items);
        }
        None
    }

    pub(super) fn emit_instance(
        &self,
        request: InstanceEmitRequest<'_>,
    ) -> Result<Vec<EirItem>, CompileError> {
        let (callable_def, callable_name, callable) =
            self.callable_from_elab(request.callee, request.env)?;
        if request.inplace {
            let item = match callable {
                ElabCallable::Cell(item) => item,
                ElabCallable::Extern(_) => {
                    return Err(CompileError::lowering_at(
                        crate::EirError::InplaceOnExternCell {
                            name: callable_name.to_string(),
                        },
                        request.span,
                    ));
                }
            };
            return self.emit_cell_inline(CellInlineRequest {
                callable_def,
                inst_name: request.inst_name,
                callable_name: &callable_name,
                item,
                callee: request.callee,
                args: request.args,
                caller_env: request.env,
            });
        }
        let safe_name = self.sanitize(&format!("{}_inst", request.inst_name));
        let params = self.generic_actuals_for_elab(callable_def, request.callee, request.env);
        let mut conns = Vec::new();
        let mut used_conns = BTreeSet::new();
        let formals =
            self.callable_params_for_elab(callable_def, &callable_name, request.callee)?;
        let mut binder = ActualFormalBinder::new(&formals);
        for arg in request.args {
            let formal = binder.resolve(&callable_name, arg.name.as_deref(), arg.span)?;
            if self.push_view_arg_connections(
                &mut conns,
                &mut used_conns,
                ViewArgConnectionRequest {
                    formal_owner: Some(callable_def),
                    formal: &formal.0,
                    actual: &arg.value,
                    ty: &formal.1,
                    env: request.env,
                },
            )? {
                continue;
            }
            self.push_conn(
                &mut conns,
                &mut used_conns,
                ConnectionPushRequest {
                    formal: &formal.0,
                    actual: self.elab_expr(&arg.value, request.env),
                    span: arg.span,
                },
            )?;
        }
        if let Some(result) = self.callable_result_for(callable_def) {
            let result_ty = self.specialize_type_for_elab(&result.ty, callable_def, request.callee);
            self.push_result_connections(
                &mut conns,
                &mut used_conns,
                ResultConnectionRequest {
                    formal_owner: Some(callable_def),
                    formal: &result.name,
                    actual: request.inst_name,
                    ty: &result_ty,
                    span: result.span,
                },
            )?;
        }
        Ok(vec![EirItem::Instance(EirInstance::new(
            callable_name,
            params,
            safe_name,
            request.inst_name,
            conns,
            request.env.origin(request.span),
        ))])
    }

    fn emit_cell_inline(
        &self,
        request: CellInlineRequest<'_>,
    ) -> Result<Vec<EirItem>, CompileError> {
        let call_span = self.call_span(request.callee, request.args);
        let mut cell_env = Env::with_prefix(self.sanitize(request.inst_name));
        cell_env.owner = Some(request.callable_def);
        cell_env.expansion_stack = request.caller_env.expansion_stack.clone();
        cell_env.push_expansion(request.callable_name, request.inst_name, call_span);
        self.insert_call_generics(
            &mut cell_env,
            request.item,
            request.callee,
            request.caller_env.owner,
        );
        self.bind_cell_args(&request, &mut cell_env)?;
        if let Some(result) = &request.item.result {
            let result_ty =
                self.specialize_type_for_elab(&result.ty, request.callable_def, request.callee);
            self.bind_cell_result(request.inst_name, &result.name, &result_ty, &mut cell_env);
        }
        self.predeclare_cell_locals(&request.item.body, &mut cell_env);
        let items = self.emit_body(&request.item.body, &mut cell_env)?;
        Ok(vec![EirItem::CellExpansion(
            crate::eir::EirCellExpansion::new(request.callable_name, request.inst_name, items),
        )])
    }

    fn call_span(&self, callee: &ElabExpr, args: &[ElabCallArg]) -> Span {
        args.iter()
            .fold(callee.span(), |span, arg| span.join(arg.span))
    }

    fn insert_call_generics(
        &self,
        env: &mut Env,
        item: &ElabCallableItem,
        callee: &ElabExpr,
        caller_owner: Option<DefId>,
    ) {
        let actuals = match &callee.node {
            ElabExprNode::GenericApp { args, .. } => args.as_slice(),
            _ => &[],
        };
        for (idx, generic) in item.generics.iter().enumerate() {
            if let Some(arg) = actuals.get(idx) {
                let actual = self.canonicalize_callsite_type(caller_owner, arg);
                env.type_replacements.insert(generic.name.clone(), actual);
            }
            let Some(kind) = &generic.kind else {
                continue;
            };
            let kind_ref = kind.clone();
            if !matches!(self.static_type_name(&kind_ref), Some("nat" | "bool")) {
                continue;
            }
            let value = actuals
                .get(idx)
                .map(|arg| self.type_arg_value(env.owner, arg))
                .or_else(|| {
                    generic
                        .default
                        .as_ref()
                        .map(|expr| self.elab_expr(expr, env))
                })
                .unwrap_or_else(|| EirExpr::ident(&generic.name));
            env.insert(&generic.name, value, kind_ref);
        }
    }

    fn predeclare_cell_locals(&self, body: &ElabBlock, env: &mut Env) {
        for stmt in &body.stmts {
            match stmt {
                ElabStmt::Signal {
                    name, ty: Some(ty), ..
                }
                | ElabStmt::Reg {
                    name, ty: Some(ty), ..
                } => {
                    let ty = self.subst_type_vars(ty, &env.type_replacements);
                    env.insert(name, EirExpr::ident(env.local_name(name)), ty);
                }
                ElabStmt::ElabIf {
                    then_block,
                    else_block,
                    ..
                } => {
                    self.predeclare_cell_locals(then_block, env);
                    if let Some(block) = else_block {
                        self.predeclare_cell_locals(block, env);
                    }
                }
                ElabStmt::ElabFor { body, .. } => self.predeclare_cell_locals(body, env),
                _ => {}
            }
        }
    }

    fn bind_cell_args(
        &self,
        request: &CellInlineRequest<'_>,
        cell_env: &mut Env,
    ) -> Result<(), CompileError> {
        let formals = self.callable_params_for_elab(
            request.callable_def,
            request.callable_name,
            request.callee,
        )?;
        let mut binder = ActualFormalBinder::new(&formals);
        for arg in request.args {
            let formal = binder.resolve(request.callable_name, arg.name.as_deref(), arg.span)?;
            self.bind_cell_arg(
                CellArgBindingRequest {
                    formal: &formal.0,
                    ty: &formal.1,
                    actual: &arg.value,
                    caller_env: request.caller_env,
                },
                cell_env,
            )?;
        }
        for param in &request.item.params {
            if !binder.is_used(&param.name) {
                return Err(CompileError::lowering_at(
                    EirError::UnknownParameter {
                        name: param.name.clone(),
                        callable: request.callable_name.to_string(),
                    },
                    param.span,
                ));
            }
        }
        Ok(())
    }

    fn bind_cell_arg(
        &self,
        request: CellArgBindingRequest<'_>,
        cell_env: &mut Env,
    ) -> Result<(), CompileError> {
        cell_env.insert(
            request.formal,
            self.elab_expr(request.actual, request.caller_env),
            request.ty.clone(),
        );
        if let Some((base, view, _)) = request.ty.view_shape()
            && let Some(interface) = self.interface_for_type(cell_env.owner, base)
            && let Some(view_decl) = interface.views.iter().find(|decl| decl.name == *view)
        {
            for field in &view_decl.fields {
                if let Some(field_ty) =
                    self.view_field_type(cell_env.owner, request.ty, &field.name)
                {
                    cell_env.insert(
                        format!("{}.{}", request.formal, field.name),
                        self.elab_actual_view_field(
                            request.actual,
                            &field.name,
                            request.caller_env,
                        ),
                        field_ty,
                    );
                }
            }
        }
        Ok(())
    }

    fn bind_cell_result(&self, actual: &str, formal: &str, ty: &MirTypeRef, cell_env: &mut Env) {
        cell_env.insert(formal, EirExpr::ident(actual), ty.clone());
        if let Some((base, view, _)) = ty.view_shape()
            && let Some(interface) = self.interface_for_type(cell_env.owner, base)
            && let Some(view_decl) = interface.views.iter().find(|decl| decl.name == *view)
        {
            for field in &view_decl.fields {
                if let Some(field_ty) = self.view_field_type(cell_env.owner, ty, &field.name) {
                    cell_env.insert(
                        format!("{formal}.{}", field.name),
                        EirExpr::ident(format!("{}_{}", actual, field.name)),
                        field_ty,
                    );
                }
            }
        }
    }

    fn push_view_arg_connections(
        &self,
        conns: &mut Vec<EirConnection>,
        used_conns: &mut BTreeSet<String>,
        request: ViewArgConnectionRequest<'_>,
    ) -> Result<bool, CompileError> {
        let Some((base, view, _)) = request.ty.view_shape() else {
            return Ok(false);
        };
        let Some(interface) = self.interface_for_type(request.formal_owner, base) else {
            return Ok(false);
        };
        let Some(view_decl) = interface.views.iter().find(|decl| decl.name == *view) else {
            return Ok(false);
        };
        for field in &view_decl.fields {
            self.push_conn(
                conns,
                used_conns,
                ConnectionPushRequest {
                    formal: &format!("{}_{}", request.formal, field.name),
                    actual: self.elab_actual_view_field(request.actual, &field.name, request.env),
                    span: request.actual.span(),
                },
            )?;
        }
        Ok(true)
    }

    fn push_result_connections(
        &self,
        conns: &mut Vec<EirConnection>,
        used_conns: &mut BTreeSet<String>,
        request: ResultConnectionRequest<'_>,
    ) -> Result<(), CompileError> {
        if let Some((base, view, _)) = request.ty.view_shape()
            && let Some(interface) = self.interface_for_type(request.formal_owner, base)
            && let Some(view_decl) = interface.views.iter().find(|decl| decl.name == *view)
        {
            for field in &view_decl.fields {
                self.push_conn(
                    conns,
                    used_conns,
                    ConnectionPushRequest {
                        formal: &format!("{}_{}", request.formal, field.name),
                        actual: EirExpr::ident(format!("{}_{}", request.actual, field.name)),
                        span: request.span,
                    },
                )?;
            }
            return Ok(());
        }
        self.push_conn(
            conns,
            used_conns,
            ConnectionPushRequest {
                formal: request.formal,
                actual: EirExpr::ident(request.actual),
                span: request.span,
            },
        )
    }

    fn push_conn(
        &self,
        conns: &mut Vec<EirConnection>,
        used_conns: &mut BTreeSet<String>,
        request: ConnectionPushRequest<'_>,
    ) -> Result<(), CompileError> {
        if !used_conns.insert(request.formal.to_string()) {
            let error = EirError::DuplicateConnection {
                name: request.formal.to_string(),
            };
            return Err(CompileError::lowering_at(error, request.span));
        }
        conns.push(EirConnection::new(request.formal, request.actual));
        Ok(())
    }
}
