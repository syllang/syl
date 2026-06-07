use std::collections::BTreeMap;

use syl_hir::DefId;
use syl_sema::ir::const_mir::{ConstNamedExpr, ConstStructKind};

use super::{ConstEvalEnv, ConstKind, ConstValue, ConstValueElaborator};
use crate::{
    CompileError, ConstEvalError, EirError, TirError,
    const_mir::{ConstExpr, ConstFunction, ConstMirProgram},
    mir::MirTypeRef,
    program::{ElabCallArg, ElabExpr, ElabExprNode, ElabNamedExpr, ElabProgram, ElabResolution},
};

impl ConstValueElaborator for ConstMirProgram {
    fn elab_value(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError> {
        let lowered = ElabConstLowerer::new(self, program, env).lower(expr)?;
        self.evaluator().expr_value(&lowered, env)
    }

    fn elab_bool(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<Option<bool>, CompileError> {
        match self.elab_value(program, expr, env)? {
            ConstValue::Bool(value) => Ok(Some(value)),
            ConstValue::Unknown(ConstKind::Bool) => Ok(None),
            ConstValue::Unknown(ConstKind::Nat) | ConstValue::Nat(_) | ConstValue::Unknown(_) => {
                Err(CompileError::lowering_at(
                    TirError::ElaborationIfRequiresBool,
                    expr.span(),
                ))
            }
            _ => Err(CompileError::lowering_at(
                TirError::ElaborationIfRequiresBool,
                expr.span(),
            )),
        }
    }

    fn require_elab_nat(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
        context: &str,
    ) -> Result<ConstValue, CompileError> {
        let value = match self.elab_value(program, expr, env) {
            Ok(value) => value,
            Err(CompileError::Lowering { kind, .. }) => {
                return Err(CompileError::lowering_at(
                    ConstEvalError::NotElaborationTimeExpression {
                        context: context.to_string(),
                        source: kind,
                    },
                    expr.span(),
                ));
            }
            Err(error) => return Err(error),
        };
        match value {
            ConstValue::Nat(_) | ConstValue::Unknown(ConstKind::Nat) => Ok(value),
            ConstValue::Bool(_) | ConstValue::Unknown(ConstKind::Bool) | ConstValue::Unknown(_) => {
                Err(CompileError::lowering_at(
                    TirError::RequiresNatExpression {
                        context: context.to_string(),
                    },
                    expr.span(),
                ))
            }
            _ => Err(CompileError::lowering_at(
                TirError::RequiresNatExpression {
                    context: context.to_string(),
                },
                expr.span(),
            )),
        }
    }

    fn kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
        self.evaluator().kind_for_type(ty)
    }
}

#[non_exhaustive]
struct ElabConstLowerer<'program, 'env> {
    const_mir: &'program ConstMirProgram,
    program: &'program ElabProgram,
    env: &'env ConstEvalEnv,
    owner: Option<DefId>,
}

impl<'program, 'env> ElabConstLowerer<'program, 'env> {
    fn new(
        const_mir: &'program ConstMirProgram,
        program: &'program ElabProgram,
        env: &'env ConstEvalEnv,
    ) -> Self {
        Self {
            const_mir,
            program,
            env,
            owner: env.owner(),
        }
    }

    fn with_owner(&self, owner: DefId) -> Self {
        Self {
            const_mir: self.const_mir,
            program: self.program,
            env: self.env,
            owner: Some(owner),
        }
    }

    fn lower(&self, expr: &ElabExpr) -> Result<ConstExpr, CompileError> {
        match &expr.node {
            ElabExprNode::Ident(name) => self.ident_expr(expr, name),
            ElabExprNode::Int(value) => Ok(ConstExpr::nat(*value, expr.span())),
            ElabExprNode::Bool(value) => Ok(ConstExpr::bool_value(*value, expr.span())),
            ElabExprNode::Group(expr) | ElabExprNode::GenericApp { callee: expr, .. } => {
                self.lower(expr)
            }
            ElabExprNode::Unary { op, expr: inner } => {
                Ok(ConstExpr::unary(*op, self.lower(inner)?, expr.span()))
            }
            ElabExprNode::Binary { op, left, right } => Ok(ConstExpr::binary(
                *op,
                self.lower(left)?,
                self.lower(right)?,
                expr.span(),
            )),
            ElabExprNode::Call { callee, args } => self.call_expr(expr, callee, args),
            ElabExprNode::Aggregate { ty, fields } => self.aggregate_expr(expr, ty, fields),
            ElabExprNode::Field { base, field } => self.field_expr(expr, base, field),
            ElabExprNode::Unsupported => Err(self.invalid(expr)),
            ElabExprNode::Str(_)
            | ElabExprNode::Index { .. }
            | ElabExprNode::Block(_)
            | ElabExprNode::Match { .. }
            | ElabExprNode::Select { .. }
            | ElabExprNode::Place { .. }
            | ElabExprNode::For { .. }
            | ElabExprNode::CompileError { .. }
            | ElabExprNode::Range { .. } => Err(self.invalid(expr)),
        }
    }

    fn ident_expr(&self, expr: &ElabExpr, name: &str) -> Result<ConstExpr, CompileError> {
        if let Some(value) = self.materialized_struct_local(expr, name) {
            return Ok(value);
        }
        if self.env.value(name).is_some() {
            return Ok(ConstExpr::named_local(name, expr.span()));
        }
        if let Some((owner, item)) = self.resolved_const(expr) {
            return self.with_owner(owner).lower(&item.value);
        }
        Err(CompileError::lowering_at(
            ConstEvalError::UnknownElaborationIdentifier {
                name: name.to_string(),
            },
            expr.span(),
        ))
    }

    fn aggregate_expr(
        &self,
        expr: &ElabExpr,
        ty: &MirTypeRef,
        fields: &[ElabNamedExpr],
    ) -> Result<ConstExpr, CompileError> {
        let Some(owner) = self.owner else {
            return Err(self.invalid(expr));
        };
        let Some(def) = self
            .program
            .expr_type(owner, expr)
            .and_then(|ty| ty.definition())
            .or_else(|| self.resolved_type_def(owner, ty))
        else {
            return Err(self.invalid(expr));
        };
        let Some(kind) = self.const_mir.struct_kind(def) else {
            return Err(self.invalid(expr));
        };
        let fields = fields
            .iter()
            .map(|field| {
                self.lower(&field.value)
                    .map(|value| ConstNamedExpr::new(field.name.clone(), value))
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ConstExpr::aggregate(kind, fields, expr.span()))
    }

    fn field_expr(
        &self,
        expr: &ElabExpr,
        base: &ElabExpr,
        field: &str,
    ) -> Result<ConstExpr, CompileError> {
        if let Some(owner) = self.owner
            && let Some(value) = self.program.enum_variant_field_value(owner, base, field)
        {
            return Ok(ConstExpr::nat(value, expr.span()));
        }
        Ok(ConstExpr::field(
            self.lower(base)?,
            field.to_string(),
            expr.span(),
        ))
    }

    fn materialized_struct_local(&self, expr: &ElabExpr, name: &str) -> Option<ConstExpr> {
        let owner = self.owner?;
        let def = self
            .program
            .expr_type(owner, expr)
            .and_then(|ty| ty.definition())
            .or_else(|| match self.program.expr_resolution(owner, expr) {
                Some(ElabResolution::Local(local)) => self
                    .program
                    .local_type(local)
                    .and_then(|ty| ty.definition()),
                _ => None,
            })?;
        let kind = self.const_mir.struct_kind(def)?;
        self.materialize_struct_fields(kind, name, expr.span())
    }

    fn materialize_struct_fields(
        &self,
        kind: ConstStructKind,
        root: &str,
        span: syl_span::Span,
    ) -> Option<ConstExpr> {
        let fields = self
            .const_mir
            .struct_def(kind.def())?
            .fields()
            .iter()
            .map(|field| {
                let field_path = format!("{root}.{}", field.name());
                let value = match field.kind() {
                    Some(ConstKind::Struct(child)) => self
                        .materialize_struct_fields(child, &field_path, span)
                        .or_else(|| {
                            self.env
                                .value(&field_path)
                                .map(|_| ConstExpr::named_local(field_path.clone(), span))
                        })?,
                    _ => self
                        .env
                        .value(&field_path)
                        .map(|_| ConstExpr::named_local(field_path.clone(), span))?,
                };
                Some(ConstNamedExpr::new(field.name().to_string(), value))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ConstExpr::aggregate(kind, fields, span))
    }

    fn resolved_const(&self, expr: &ElabExpr) -> Option<(DefId, &crate::program::ElabConstItem)> {
        let owner = self.owner?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, expr) else {
            return None;
        };
        self.program.const_by_def(def).map(|item| (def, item))
    }

    fn call_expr(
        &self,
        expr: &ElabExpr,
        callee: &ElabExpr,
        args: &[ElabCallArg],
    ) -> Result<ConstExpr, CompileError> {
        let Some(def) = self.const_function_def(callee) else {
            return Err(CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier {
                    name: CalleeName::new(callee).resolve(),
                },
                callee.span(),
            ));
        };
        let Some(function) = self.const_mir.function(def) else {
            return Err(CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier {
                    name: self
                        .program
                        .def_name(def)
                        .unwrap_or("<unknown>")
                        .to_string(),
                },
                callee.span(),
            ));
        };
        Ok(ConstExpr::call(
            def,
            self.call_args(function, args)?,
            expr.span(),
        ))
    }

    fn call_args(
        &self,
        function: &ConstFunction,
        args: &[ElabCallArg],
    ) -> Result<Vec<ConstExpr>, CompileError> {
        let mut values = ElabConstArgBinder::new(self, function, args).bind()?;
        let mut out = Vec::new();
        for param in function.params() {
            if let Some(value) = values.remove(param) {
                out.push(value);
            }
        }
        Ok(out)
    }

    fn const_function_def(&self, callee: &ElabExpr) -> Option<DefId> {
        let owner = self.owner?;
        let root = CalleeRoot::new(callee).resolve()?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, root) else {
            return None;
        };
        Some(def)
    }

    fn resolved_type_def(&self, owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
        if let Some(path) = ty.path() {
            return match path {
                [name] => self.program.resolve_def_id(owner, name),
                _ => self.program.canonical_def_id(path),
            };
        }
        if let Some(base) = ty.generic_base() {
            return self.resolved_type_def(owner, base);
        }
        if let Some((base, _)) = ty.view_select() {
            return self.resolved_type_def(owner, base);
        }
        if let Some((_, elem)) = ty.array() {
            return self.resolved_type_def(owner, elem);
        }
        None
    }

    fn invalid(&self, expr: &ElabExpr) -> CompileError {
        CompileError::lowering_at(ConstEvalError::InvalidElaborationExpression, expr.span())
    }
}

#[non_exhaustive]
struct ElabConstArgBinder<'program, 'env, 'args> {
    lowerer: &'args ElabConstLowerer<'program, 'env>,
    function: &'args ConstFunction,
    args: &'args [ElabCallArg],
    values: BTreeMap<String, ConstExpr>,
    next_positional: usize,
}

impl<'program, 'env, 'args> ElabConstArgBinder<'program, 'env, 'args> {
    fn new(
        lowerer: &'args ElabConstLowerer<'program, 'env>,
        function: &'args ConstFunction,
        args: &'args [ElabCallArg],
    ) -> Self {
        Self {
            lowerer,
            function,
            args,
            values: BTreeMap::new(),
            next_positional: 0,
        }
    }

    fn bind(mut self) -> Result<BTreeMap<String, ConstExpr>, CompileError> {
        for arg in self.args {
            let param = self.resolve_param(arg)?.to_string();
            self.values.insert(param, self.lowerer.lower(&arg.value)?);
        }
        Ok(self.values)
    }

    fn resolve_param(&mut self, arg: &ElabCallArg) -> Result<&str, CompileError> {
        if let Some(name) = &arg.name {
            return self
                .function
                .params()
                .iter()
                .find(|param| *param == name)
                .map(String::as_str)
                .ok_or_else(|| self.unknown_parameter(arg));
        }
        let param = self
            .function
            .params()
            .get(self.next_positional)
            .map(String::as_str)
            .ok_or_else(|| self.unknown_parameter(arg))?;
        self.next_positional += 1;
        Ok(param)
    }

    fn unknown_parameter(&self, arg: &ElabCallArg) -> CompileError {
        CompileError::lowering_at(
            EirError::UnknownParameter {
                name: arg
                    .name
                    .clone()
                    .unwrap_or_else(|| "<positional>".to_string()),
                callable: self.function.name().to_string(),
            },
            arg.span,
        )
    }
}

#[non_exhaustive]
struct CalleeRoot<'a> {
    callee: &'a ElabExpr,
}

impl<'a> CalleeRoot<'a> {
    fn new(callee: &'a ElabExpr) -> Self {
        Self { callee }
    }

    fn resolve(&self) -> Option<&'a ElabExpr> {
        let mut current = self.callee;
        loop {
            match &current.node {
                ElabExprNode::Ident(_) => return Some(current),
                ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }
}

#[non_exhaustive]
struct CalleeName<'a> {
    callee: &'a ElabExpr,
}

impl<'a> CalleeName<'a> {
    fn new(callee: &'a ElabExpr) -> Self {
        Self { callee }
    }

    fn resolve(&self) -> String {
        CalleeRoot::new(self.callee)
            .resolve()
            .and_then(|expr| match &expr.node {
                ElabExprNode::Ident(name) => Some(name.clone()),
                _ => None,
            })
            .unwrap_or_else(|| "<expression>".to_string())
    }
}
