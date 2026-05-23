use super::{ConstExpr, ConstFunction, ConstMirProgram};
use crate::{
    CompileError, ConstEvalError, EirError, TirError,
    const_eval::{ConstEvalEnv, ConstKind, ConstValue},
    program::{ElabExpr, ElabExprNode, ElabInstArg, ElabProgram, ElabResolution},
};
use std::collections::BTreeMap;
use syl_hir::DefId;

impl ConstMirProgram {
    pub(crate) fn elab_value(
        &self,
        program: &ElabProgram,
        expr: &ElabExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError> {
        let lowered = ElabConstLowerer::new(self, program, env).lower(expr)?;
        self.evaluator().expr_value(&lowered, env)
    }

    pub(crate) fn elab_bool(
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

    pub(crate) fn require_elab_nat(
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
}

#[non_exhaustive]
struct ElabConstLowerer<'program, 'env> {
    const_mir: &'program ConstMirProgram,
    program: &'program ElabProgram,
    env: &'env ConstEvalEnv,
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
            ElabExprNode::Unsupported => Err(self.invalid(expr)),
            ElabExprNode::Str(_)
            | ElabExprNode::Aggregate { .. }
            | ElabExprNode::Field { .. }
            | ElabExprNode::Index { .. }
            | ElabExprNode::Block(_)
            | ElabExprNode::Match { .. }
            | ElabExprNode::Select { .. }
            | ElabExprNode::Inst { .. }
            | ElabExprNode::CompileError { .. }
            | ElabExprNode::Range { .. } => Err(self.invalid(expr)),
        }
    }

    fn ident_expr(&self, expr: &ElabExpr, name: &str) -> Result<ConstExpr, CompileError> {
        if self.env.value(name).is_some() {
            return Ok(ConstExpr::named_local(name, expr.span()));
        }
        if let Some(item) = self.resolved_const(expr) {
            return self.lower(&item.value);
        }
        if let Some(value) = self
            .program
            .enum_variant_value_by_name(self.env.owner(), name)
        {
            return Ok(ConstExpr::nat(value, expr.span()));
        }
        Err(CompileError::lowering_at(
            ConstEvalError::UnknownElaborationIdentifier {
                name: name.to_string(),
            },
            expr.span(),
        ))
    }

    fn resolved_const(&self, expr: &ElabExpr) -> Option<&crate::program::ElabConstItem> {
        let owner = self.env.owner()?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, expr) else {
            return None;
        };
        self.program.const_by_def(def)
    }

    fn call_expr(
        &self,
        expr: &ElabExpr,
        callee: &ElabExpr,
        args: &[ElabInstArg],
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
        args: &[ElabInstArg],
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
        let owner = self.env.owner()?;
        let root = CalleeRoot::new(callee).resolve()?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, root) else {
            return None;
        };
        Some(def)
    }

    fn invalid(&self, expr: &ElabExpr) -> CompileError {
        CompileError::lowering_at(ConstEvalError::InvalidElaborationExpression, expr.span())
    }
}

#[non_exhaustive]
struct ElabConstArgBinder<'program, 'env, 'args> {
    lowerer: &'args ElabConstLowerer<'program, 'env>,
    function: &'args ConstFunction,
    args: &'args [ElabInstArg],
    values: BTreeMap<String, ConstExpr>,
    next_positional: usize,
}

impl<'program, 'env, 'args> ElabConstArgBinder<'program, 'env, 'args> {
    fn new(
        lowerer: &'args ElabConstLowerer<'program, 'env>,
        function: &'args ConstFunction,
        args: &'args [ElabInstArg],
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

    fn resolve_param(&mut self, arg: &ElabInstArg) -> Result<&str, CompileError> {
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

    fn unknown_parameter(&self, arg: &ElabInstArg) -> CompileError {
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
        let Some(root) = CalleeRoot::new(self.callee).resolve() else {
            return "<unknown>".to_string();
        };
        let ElabExprNode::Ident(name) = &root.node else {
            return "<unknown>".to_string();
        };
        name.clone()
    }
}
