use crate::{
    CompileError,
    const_eval::{ConstEvalEnv, ConstValue},
    eir_build::{EirBuilder, Env, VarInfo},
    eir_expr::EirExpr,
    program::ElabExpr,
};

impl<'a> EirBuilder<'a> {
    pub(super) fn elab_const_value(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<ConstValue, CompileError> {
        self.const_mir
            .elab_value(self.program, expr, &mut self.const_eval_env(env))
    }

    pub(super) fn elab_const_bool(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<Option<bool>, CompileError> {
        self.const_mir
            .elab_bool(self.program, expr, &mut self.const_eval_env(env))
    }

    pub(super) fn elab_require_const_nat(
        &self,
        expr: &ElabExpr,
        env: &Env,
        context: &str,
    ) -> Result<ConstValue, CompileError> {
        self.const_mir
            .require_elab_nat(self.program, expr, &mut self.const_eval_env(env), context)
    }

    fn const_eval_env(&self, env: &Env) -> ConstEvalEnv {
        let mut out = ConstEvalEnv::with_owner(env.owner);
        for (name, var) in &env.vars {
            if let Some(value) = self.const_value_for_var(var) {
                out.bind(name.clone(), value);
            }
        }
        out
    }

    fn const_value_for_var(&self, var: &VarInfo) -> Option<ConstValue> {
        match &var.code {
            EirExpr::Int(value) => Some(ConstValue::Int(*value)),
            EirExpr::Bool(value) => Some(ConstValue::Bool(*value)),
            _ => self
                .const_mir
                .evaluator()
                .kind_for_type(&var.ty)
                .map(ConstValue::Unknown),
        }
    }
}
