use crate::{
    CompileError, ConstEvalError, LoweringError, TirError,
    ir::{
        const_mir::{
            BuiltinConstTypeOracle, ConstExpr, ConstExprKind, ConstFunction, ConstFunctionStore,
            ConstMirProgram, ConstStmt, ConstTypeOracle, Terminator,
        },
        mir::{MirBinaryOp, MirTypeRef, MirUnaryOp},
    },
};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId};
use syl_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ConstKind {
    Nat,
    Bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum ConstValue {
    Unknown(ConstKind),
    Nat(u64),
    Bool(bool),
}

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ConstEvalEnv {
    bindings: BTreeMap<String, ConstValue>,
    owner: Option<DefId>,
}

impl ConstEvalEnv {
    pub fn with_owner(owner: Option<DefId>) -> Self {
        Self {
            bindings: BTreeMap::new(),
            owner,
        }
    }

    pub fn bind(&mut self, name: impl Into<String>, value: ConstValue) {
        self.bindings.insert(name.into(), value);
    }

    pub fn value(&self, name: &str) -> Option<ConstValue> {
        self.bindings.get(name).copied()
    }

    pub fn owner(&self) -> Option<DefId> {
        self.owner
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct ConstCallKey {
    callee: DefId,
    args: Vec<ConstValue>,
}

#[non_exhaustive]
pub struct ConstEvaluator<'program> {
    program: &'program dyn ConstFunctionStore,
    oracle: &'program dyn ConstTypeOracle,
    call_stack: Vec<String>,
    step_limit: usize,
    remaining_steps: usize,
    call_cache: BTreeMap<ConstCallKey, ConstValue>,
    expr_values: BTreeMap<ExprId, ConstValue>,
}

impl<'program> ConstEvaluator<'program> {
    pub fn new(program: &'program ConstMirProgram) -> Self {
        static BUILTIN_ORACLE: BuiltinConstTypeOracle = BuiltinConstTypeOracle;
        Self::with_dependencies(program, &BUILTIN_ORACLE)
    }

    fn with_dependencies(
        program: &'program dyn ConstFunctionStore,
        oracle: &'program dyn ConstTypeOracle,
    ) -> Self {
        let step_limit = 10_000;
        Self {
            program,
            oracle,
            call_stack: Vec::new(),
            step_limit,
            remaining_steps: step_limit,
            call_cache: BTreeMap::new(),
            expr_values: BTreeMap::new(),
        }
    }

    pub fn kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
        self.oracle.const_kind_for_type(ty)
    }

    pub fn expr_value(
        &mut self,
        expr: &ConstExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError> {
        self.call_stack.clear();
        self.remaining_steps = self.step_limit;
        self.expr_values.clear();
        self.mir_expr_value(expr, env)
    }

    pub fn recorded_expr_values(&self) -> &BTreeMap<ExprId, ConstValue> {
        &self.expr_values
    }

    fn binary_result(
        &self,
        op: MirBinaryOp,
        lhs: ConstValue,
        rhs: ConstValue,
        span: Span,
    ) -> Result<ConstValue, CompileError> {
        if matches!(lhs, ConstValue::Unknown(_)) || matches!(rhs, ConstValue::Unknown(_)) {
            return self.unknown_binary_value(op, lhs, rhs, span);
        }
        match (op, lhs, rhs) {
            (MirBinaryOp::Eq, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Bool(a == b))
            }
            (MirBinaryOp::Eq, ConstValue::Bool(a), ConstValue::Bool(b)) => {
                Ok(ConstValue::Bool(a == b))
            }
            (MirBinaryOp::NotEq, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Bool(a != b))
            }
            (MirBinaryOp::NotEq, ConstValue::Bool(a), ConstValue::Bool(b)) => {
                Ok(ConstValue::Bool(a != b))
            }
            (MirBinaryOp::Lt, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Bool(a < b))
            }
            (MirBinaryOp::LtEq, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Bool(a <= b))
            }
            (MirBinaryOp::Gt, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Bool(a > b))
            }
            (MirBinaryOp::GtEq, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Bool(a >= b))
            }
            (MirBinaryOp::AndAnd, ConstValue::Bool(a), ConstValue::Bool(b)) => {
                Ok(ConstValue::Bool(a && b))
            }
            (MirBinaryOp::OrOr, ConstValue::Bool(a), ConstValue::Bool(b)) => {
                Ok(ConstValue::Bool(a || b))
            }
            (MirBinaryOp::Add, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Nat(a + b))
            }
            (MirBinaryOp::Sub, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Nat(a.saturating_sub(b)))
            }
            (MirBinaryOp::Mul, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Nat(a * b))
            }
            (MirBinaryOp::Div, ConstValue::Nat(a), ConstValue::Nat(b)) if b != 0 => {
                Ok(ConstValue::Nat(a / b))
            }
            (MirBinaryOp::Rem, ConstValue::Nat(a), ConstValue::Nat(b)) if b != 0 => {
                Ok(ConstValue::Nat(a % b))
            }
            (MirBinaryOp::Shl, ConstValue::Nat(a), ConstValue::Nat(b)) => {
                Ok(ConstValue::Nat(a << b))
            }
            _ => Err(self.const_error(ConstEvalError::InvalidConstBinaryExpression, span)),
        }
    }

    fn unknown_binary_value(
        &self,
        op: MirBinaryOp,
        lhs: ConstValue,
        rhs: ConstValue,
        span: Span,
    ) -> Result<ConstValue, CompileError> {
        let lhs_kind = self.kind_of(lhs);
        let rhs_kind = self.kind_of(rhs);
        match op {
            MirBinaryOp::Eq | MirBinaryOp::NotEq => {
                if lhs_kind == rhs_kind {
                    Ok(ConstValue::Unknown(ConstKind::Bool))
                } else {
                    Err(self.const_error(ConstEvalError::ConstEqualityTypeMismatch, span))
                }
            }
            MirBinaryOp::Lt | MirBinaryOp::LtEq | MirBinaryOp::Gt | MirBinaryOp::GtEq => {
                self.require_kind(lhs_kind, ConstKind::Nat, span)?;
                self.require_kind(rhs_kind, ConstKind::Nat, span)?;
                Ok(ConstValue::Unknown(ConstKind::Bool))
            }
            MirBinaryOp::AndAnd | MirBinaryOp::OrOr => {
                self.require_kind(lhs_kind, ConstKind::Bool, span)?;
                self.require_kind(rhs_kind, ConstKind::Bool, span)?;
                Ok(ConstValue::Unknown(ConstKind::Bool))
            }
            MirBinaryOp::Add
            | MirBinaryOp::Sub
            | MirBinaryOp::Mul
            | MirBinaryOp::Div
            | MirBinaryOp::Rem
            | MirBinaryOp::Shl => {
                self.require_kind(lhs_kind, ConstKind::Nat, span)?;
                self.require_kind(rhs_kind, ConstKind::Nat, span)?;
                Ok(ConstValue::Unknown(ConstKind::Nat))
            }
            _ => Err(self.const_error(ConstEvalError::InvalidConstBinaryExpression, span)),
        }
    }

    fn eval_function(
        &mut self,
        function: &ConstFunction,
        values: BTreeMap<String, ConstValue>,
    ) -> Result<ConstValue, CompileError> {
        if function.is_unsupported() {
            return Err(CompileError::lowering_at(
                ConstEvalError::InvalidElaborationExpression,
                function
                    .unsupported_span()
                    .unwrap_or_else(|| function.span()),
            ));
        }
        self.call_stack.push(function.name().to_string());
        let mut env = ConstEvalEnv::with_owner(Some(function.def()));
        for (name, value) in values {
            env.bind(name, value);
        }
        let block = function.block(function.entry()).ok_or_else(|| {
            CompileError::lowering_at(
                ConstEvalError::InvalidElaborationExpression,
                function.span(),
            )
        })?;
        let _entry_exists = block;
        let result = self.execute_function(function, &mut env);
        self.call_stack.pop();
        result
    }

    fn execute_function(
        &mut self,
        function: &ConstFunction,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError> {
        let mut current = function.entry();
        loop {
            self.consume_step(function.span())?;
            let block = function.block(current).ok_or_else(|| {
                CompileError::lowering_at(
                    ConstEvalError::InvalidElaborationExpression,
                    function.span(),
                )
            })?;
            for stmt in block.stmts() {
                self.eval_stmt(stmt, env)?;
            }
            match block.terminator() {
                Terminator::Goto(target) => current = *target,
                Terminator::Branch {
                    cond,
                    then_block,
                    else_block,
                } => match self.mir_expr_value(cond, env)? {
                    ConstValue::Bool(true) => current = *then_block,
                    ConstValue::Bool(false) => current = *else_block,
                    ConstValue::Unknown(ConstKind::Bool) => {
                        return Err(CompileError::lowering_at(
                            ConstEvalError::InvalidElaborationExpression,
                            cond.span(),
                        ));
                    }
                    ConstValue::Unknown(ConstKind::Nat) | ConstValue::Nat(_) => {
                        return Err(CompileError::lowering_at(
                            TirError::ElaborationIfRequiresBool,
                            cond.span(),
                        ));
                    }
                },
                Terminator::Return(Some(expr)) => return self.mir_expr_value(expr, env),
                Terminator::Return(None) => {
                    return Err(CompileError::lowering_at(
                        ConstEvalError::InvalidElaborationExpression,
                        function.span(),
                    ));
                }
            }
        }
    }

    fn eval_stmt(&mut self, stmt: &ConstStmt, env: &mut ConstEvalEnv) -> Result<(), CompileError> {
        match stmt {
            ConstStmt::Assign { local, value } => {
                self.consume_step(value.span())?;
                let value = self.mir_expr_value(value, env)?;
                env.bind(local.name(), value);
                Ok(())
            }
        }
    }

    fn mir_expr_value(
        &mut self,
        expr: &ConstExpr,
        env: &mut ConstEvalEnv,
    ) -> Result<ConstValue, CompileError> {
        self.consume_step(expr.span())?;
        let value = match expr.kind() {
            ConstExprKind::Local(local) => env.value(local.name()).ok_or_else(|| {
                CompileError::lowering_at(
                    ConstEvalError::UnknownElaborationIdentifier {
                        name: local.name().to_string(),
                    },
                    expr.span(),
                )
            }),
            ConstExprKind::Unknown(kind) => Ok(ConstValue::Unknown(*kind)),
            ConstExprKind::Nat(value) => Ok(ConstValue::Nat(*value)),
            ConstExprKind::Bool(value) => Ok(ConstValue::Bool(*value)),
            ConstExprKind::Unary { op, expr: inner } => {
                let value = self.mir_expr_value(inner, env)?;
                match (*op, value) {
                    (MirUnaryOp::Not, ConstValue::Bool(value)) => Ok(ConstValue::Bool(!value)),
                    (MirUnaryOp::Not, ConstValue::Unknown(ConstKind::Bool)) => {
                        Ok(ConstValue::Unknown(ConstKind::Bool))
                    }
                    _ => Err(CompileError::lowering_at(
                        ConstEvalError::InvalidConstUnaryExpression,
                        expr.span(),
                    )),
                }
            }
            ConstExprKind::Binary { op, left, right } => {
                let lhs = self.mir_expr_value(left, env)?;
                // Short-circuit &&: LHS = false
                if *op == MirBinaryOp::AndAnd && matches!(lhs, ConstValue::Bool(false)) {
                    if let Some(origin) = expr.origin() {
                        self.expr_values.insert(origin, lhs);
                    }
                    return Ok(lhs);
                }
                // Short-circuit ||: LHS = true
                if *op == MirBinaryOp::OrOr && matches!(lhs, ConstValue::Bool(true)) {
                    if let Some(origin) = expr.origin() {
                        self.expr_values.insert(origin, lhs);
                    }
                    return Ok(lhs);
                }
                let rhs = self.mir_expr_value(right, env)?;
                self.binary_result(*op, lhs, rhs, expr.span())
            }
            ConstExprKind::Call { callee, args } => {
                let Some(function) = self.program.function(*callee) else {
                    let name = format!("def#{}", callee.get());
                    return Err(CompileError::lowering_at(
                        ConstEvalError::UnknownElaborationIdentifier { name },
                        expr.span(),
                    ));
                };
                let mut values = BTreeMap::new();
                let mut arg_values = Vec::new();
                for (param, arg) in function.params().iter().zip(args) {
                    let value = self.mir_expr_value(arg, env)?;
                    arg_values.push(value);
                    values.insert(param.clone(), value);
                }
                if values
                    .values()
                    .any(|value| matches!(value, ConstValue::Unknown(_)))
                {
                    function.ret_kind().map(ConstValue::Unknown).ok_or_else(|| {
                        CompileError::lowering_at(
                            ConstEvalError::InvalidElaborationExpression,
                            expr.span(),
                        )
                    })
                } else {
                    let cache_key = ConstCallKey {
                        callee: *callee,
                        args: arg_values,
                    };
                    if let Some(value) = self.call_cache.get(&cache_key).copied() {
                        Ok(value)
                    } else {
                        let value = self.eval_function(function, values)?;
                        self.call_cache.insert(cache_key, value);
                        Ok(value)
                    }
                }
            }
            ConstExprKind::Unsupported => {
                return Err(CompileError::lowering_at(
                    ConstEvalError::InvalidElaborationExpression,
                    expr.span(),
                ));
            }
        }?;
        if let Some(origin) = expr.origin() {
            self.expr_values.insert(origin, value);
        }
        Ok(value)
    }

    fn consume_step(&mut self, span: Span) -> Result<(), CompileError> {
        if self.remaining_steps == 0 {
            return Err(CompileError::lowering_at(
                ConstEvalError::StepLimitExceeded {
                    limit: self.step_limit,
                },
                span,
            ));
        }
        self.remaining_steps -= 1;
        Ok(())
    }

    fn kind_of(&self, value: ConstValue) -> ConstKind {
        match value {
            ConstValue::Bool(_) => ConstKind::Bool,
            ConstValue::Nat(_) => ConstKind::Nat,
            ConstValue::Unknown(kind) => kind,
        }
    }

    fn require_kind(
        &self,
        actual: ConstKind,
        expected: ConstKind,
        span: Span,
    ) -> Result<(), CompileError> {
        if actual == expected {
            Ok(())
        } else {
            Err(self.const_error(ConstEvalError::ConstComparisonTypeMismatch, span))
        }
    }

    fn const_error(&self, kind: impl Into<LoweringError>, span: Span) -> CompileError {
        CompileError::lowering_at(kind, span)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{BasicBlock, BlockId, ConstFunctionParts};
    use super::*;
    use crate::ir::mir::MirTypeRef;
    use syl_hir::DefId;

    #[test]
    fn evaluator_uses_custom_type_oracle() {
        struct FakeOracle;

        impl ConstTypeOracle for FakeOracle {
            fn const_kind_for_type(&self, ty: &MirTypeRef) -> Option<ConstKind> {
                match ty.type_name() {
                    Some("Token") => Some(ConstKind::Bool),
                    _ => None,
                }
            }
        }

        let oracle = FakeOracle;
        let evaluator = ConstEvaluator::with_dependencies(&NoopFunctionStore, &oracle);
        let ty = MirTypeRef::path_type(vec!["Token".to_string()], Span::new(0, 1));

        assert_eq!(evaluator.kind_for_type(&ty), Some(ConstKind::Bool));
    }

    #[test]
    fn evaluator_uses_custom_function_store() {
        struct FakeFunctionStore {
            def: DefId,
            function: ConstFunction,
        }

        impl ConstFunctionStore for FakeFunctionStore {
            fn function(&self, def: DefId) -> Option<&ConstFunction> {
                if def == self.def {
                    Some(&self.function)
                } else {
                    None
                }
            }
        }

        struct NoopOracle;

        impl ConstTypeOracle for NoopOracle {
            fn const_kind_for_type(&self, _ty: &MirTypeRef) -> Option<ConstKind> {
                None
            }
        }

        let def = DefId::new(7);
        let span = Span::new(0, 1);
        let function = ConstFunction::new(ConstFunctionParts {
            def,
            name: "answer".to_string(),
            params: Vec::new(),
            ret_kind: Some(ConstKind::Nat),
            locals: Vec::new(),
            blocks: vec![BasicBlock::new(
                BlockId::new(0),
                Vec::new(),
                Terminator::Return(Some(ConstExpr::nat(7, span))),
            )],
            entry: BlockId::new(0),
            span,
            unsupported: false,
            unsupported_span: None,
        });
        let store = FakeFunctionStore { def, function };
        let oracle = NoopOracle;
        let mut evaluator = ConstEvaluator::with_dependencies(&store, &oracle);
        let expr = ConstExpr::call(def, Vec::new(), span);
        let mut env = ConstEvalEnv::with_owner(None);

        assert_eq!(
            evaluator
                .expr_value(&expr, &mut env)
                .expect("fake function store must resolve the call"),
            ConstValue::Nat(7)
        );
    }

    struct NoopFunctionStore;

    impl ConstFunctionStore for NoopFunctionStore {
        fn function(&self, _def: DefId) -> Option<&ConstFunction> {
            None
        }
    }
}
