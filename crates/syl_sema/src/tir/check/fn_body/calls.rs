use super::{FnTypeEnv, Phase, TirType, TypePhaseChecker};
use crate::hir::view::HirDesignViewExt;
use crate::{
    ActualFormalBinder, CompileError, TirError,
    hir::{HirBodyExpr, HirCallArg, HirExprNode, resolve::HirResolution},
};

fn root(expr: &HirBodyExpr) -> &HirBodyExpr {
    match &expr.node {
        HirExprNode::Group(inner) | HirExprNode::GenericApp { callee: inner, .. } => root(inner),
        _ => expr,
    }
}

impl TypePhaseChecker {
    pub(super) fn check_fn_call(
        &mut self,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
        env: &FnTypeEnv,
        errors: &mut Vec<CompileError>,
    ) -> Result<TirType, CompileError> {
        let owner = self.current_owner()?;
        let root = root(callee);
        let mut actuals = Vec::new();
        let def = if let HirExprNode::Field { base, .. } = &root.node {
            let ty = self.check_fn_expr(base, env, errors)?;
            actuals.push((None, base.as_ref(), ty));
            Self::record_recoverable(errors, self.checked_extension_method_call(owner, callee))
                .flatten()
                .map(|call| call.method)
        } else {
            match self.hir.expr_resolution(owner, root).ok().flatten() {
                Some(HirResolution::Def(def)) => Some(def),
                _ => None,
            }
        };
        for arg in args {
            let ty = self.check_fn_expr(&arg.value, env, errors)?;
            actuals.push((arg.name.as_deref(), &arg.value, ty));
        }
        let ty = if let Some(def) = def {
            if let Some(item) = self.hir.fns().get(&def) {
                let mut binder = ActualFormalBinder::new(&item.params);
                for (name, expr, actual) in &actuals {
                    if let Some(param) = Self::record_recoverable(
                        errors,
                        binder.resolve(&item.name, *name, expr.span()),
                    ) && let Some(expected) = Self::record_recoverable(
                        errors,
                        self.type_from_mir_type_ref(def, &param.ty),
                    ) {
                        self.check_fn_type(&expected, actual, expr, errors);
                    }
                }
                for param in &item.params {
                    if !binder.is_used(&param.name) {
                        errors.push(CompileError::lowering_at(
                            TirError::TypeMismatch {
                                expected: format!("argument `{}`", param.name),
                                actual: "missing argument".to_string(),
                            },
                            callee.span(),
                        ));
                    }
                }
                item.ret_ty
                    .as_ref()
                    .and_then(|ret| self.type_from_mir_type_ref(def, &ret.ty).ok())
                    .unwrap_or(TirType::Unknown)
            } else {
                errors.push(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    callee.span(),
                ));
                TirType::Unknown
            }
        } else if matches!(&root.node, HirExprNode::Ident(name) if name == "clog2")
            && self
                .hir
                .expr_resolution(owner, root)
                .ok()
                .flatten()
                .is_none()
        {
            if actuals.len() != 1 || actuals.iter().any(|(name, _, _)| name.is_some()) {
                errors.push(CompileError::lowering_at(
                    TirError::TypeMismatch {
                        expected: "one positional nat argument".to_string(),
                        actual: format!("{} arguments", actuals.len()),
                    },
                    callee.span(),
                ));
            }
            for (_, expr, actual) in &actuals {
                self.check_fn_type(&TirType::Nat, actual, expr, errors);
            }
            TirType::Nat
        } else {
            errors.push(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                callee.span(),
            ));
            TirType::Unknown
        };
        self.record_fn_callee(callee, &ty)?;
        Ok(ty)
    }

    pub(super) fn fn_expr_is_void(&self, expr: &HirBodyExpr) -> bool {
        match &expr.node {
            HirExprNode::Group(inner) => return self.fn_expr_is_void(inner),
            HirExprNode::Block(block) if !Self::fn_expr_definitely_returns(expr) => {
                return block
                    .tail
                    .as_ref()
                    .is_none_or(|tail| self.fn_expr_is_void(tail));
            }
            _ => {}
        }
        let HirExprNode::Call { callee, .. } = &expr.node else {
            return false;
        };
        let Ok(owner) = self.current_owner() else {
            return false;
        };
        let def = match self.hir.expr_resolution(owner, root(callee)).ok().flatten() {
            Some(HirResolution::Def(def)) => Some(def),
            _ => self
                .extension_method_call(owner, callee)
                .map(|call| call.method),
        };
        def.and_then(|def| self.hir.fns().get(&def))
            .is_some_and(|item| item.ret_ty.is_none())
    }

    fn record_fn_callee(&mut self, callee: &HirBodyExpr, ty: &TirType) -> Result<(), CompileError> {
        self.record_expr_type(callee, ty.clone())?;
        self.record_phase(callee, Phase::Const)?;
        if let HirExprNode::Group(inner) | HirExprNode::GenericApp { callee: inner, .. } =
            &callee.node
        {
            self.record_fn_callee(inner, ty)?;
        }
        Ok(())
    }
}
