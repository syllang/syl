use crate::{
    CompileError, ConstEvalError, EirError,
    eir::EirParamBind,
    eir_build::{EirBuilder, Env},
    eir_expr::{EirBinaryOp, EirBound, EirExpr, EirSelectArm, EirSelectMode},
    mir::{MirPattern, MirSelectMode, MirTypeRef},
    program::{
        ElabCallArg, ElabCallable, ElabExpr, ElabExprNode, ElabMatchArm, ElabNamedExpr,
        ElabProgram, ElabResolution, ElabSelectArm, ElabSignatureResultBinding,
    },
};
use std::collections::HashMap;
use syl_hir::DefId;

impl<'a> EirBuilder<'a> {
    pub(super) fn elab_call_expr(
        &self,
        callee: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> EirExpr {
        match EirBuiltinResolver::new(self.program, env.owner).resolve_call_callee(callee) {
            Some(EirBuiltinIntrinsic::HighZ) => return EirExpr::high_z(),
            Some(EirBuiltinIntrinsic::Zero) => return EirExpr::zero(),
            _ => {}
        }
        if self.map_callee_from_elab(callee, env).is_some() {
            return self.map_call_expr_from_elab(callee, args, env);
        }
        if let Some(expr) = self.extension_map_call_expr(callee, args, env) {
            return expr;
        }
        let Some(name) = self.elab_expr_name(callee) else {
            return EirExpr::unsupported("call callee is not a name");
        };
        EirExpr::call(
            name,
            args.iter()
                .map(|arg| self.elab_expr(&arg.value, env))
                .collect(),
        )
    }

    pub(super) fn elab_aggregate_expr(
        &self,
        ty: &MirTypeRef,
        fields: &[ElabNamedExpr],
        env: &Env,
    ) -> EirExpr {
        let Some(bundle) = self.bundle_for_type(env.owner, ty) else {
            return EirExpr::unsupported("aggregate type is not a known bundle");
        };
        let mut provided = HashMap::new();
        for field in fields {
            provided.entry(field.name.as_str()).or_insert(field);
        }
        let mut parts = Vec::new();
        for field in &bundle.fields {
            if let Some(value) = provided.get(field.name.as_str()) {
                parts.push(self.elab_expr(&value.value, env));
            } else {
                parts.push(EirExpr::unsupported(format!(
                    "missing aggregate field {}",
                    field.name
                )));
            }
        }
        EirExpr::Concat(parts)
    }

    pub(super) fn elab_match_expr(
        &self,
        target: &ElabExpr,
        arms: &[ElabMatchArm],
        env: &Env,
    ) -> EirExpr {
        let mut fallback = None;
        for arm in arms.iter().rev() {
            let value = self.elab_expr(&arm.value, env);
            match &arm.pattern {
                MirPattern::Wildcard(_) => fallback = Some(value),
                MirPattern::Ident(name, _) if name == "default" => fallback = Some(value),
                pattern => {
                    let cond = self.elab_match_pattern_condition(target, pattern, env);
                    fallback = Some(match fallback {
                        Some(next) => EirExpr::mux(cond, value, next),
                        None => value,
                    });
                }
            }
        }
        fallback.unwrap_or_else(|| EirExpr::unsupported("empty match expression"))
    }

    fn elab_match_pattern_condition(
        &self,
        target: &ElabExpr,
        pattern: &MirPattern,
        env: &Env,
    ) -> EirExpr {
        EirExpr::binary(
            EirBinaryOp::Eq,
            self.elab_expr(target, env),
            self.elab_match_pattern_value(pattern, env),
        )
    }

    fn elab_match_pattern_value(&self, pattern: &MirPattern, env: &Env) -> EirExpr {
        match pattern {
            MirPattern::Path(path, _) => path
                .last()
                .map(|variant| {
                    env.owner
                        .and_then(|owner| self.program.enum_variant_value(owner, path))
                        .map(EirExpr::Int)
                        .unwrap_or_else(|| EirExpr::ident(variant))
                })
                .unwrap_or_else(|| EirExpr::unsupported("empty match path pattern")),
            MirPattern::Ident(name, _) => env
                .owner
                .and_then(|owner| self.program.enum_variant_value_by_name(Some(owner), name))
                .map(EirExpr::Int)
                .unwrap_or_else(|| EirExpr::ident(name)),
            MirPattern::Int(value, _) => EirExpr::Int(*value),
            MirPattern::Bool(value, _) => EirExpr::Bool(*value),
            MirPattern::Wildcard(_) => EirExpr::unsupported("wildcard is not a condition"),
            MirPattern::Unsupported(_) => EirExpr::unsupported("unsupported match pattern"),
            _ => EirExpr::unsupported("unsupported match pattern"),
        }
    }

    pub(super) fn elab_select_expr(
        &self,
        mode: MirSelectMode,
        arms: &[ElabSelectArm],
        env: &Env,
    ) -> EirExpr {
        let mode = match mode {
            MirSelectMode::Priority => EirSelectMode::Priority,
            MirSelectMode::Unique => EirSelectMode::Unique,
            _ => return EirExpr::unsupported("unsupported select mode"),
        };
        let mut select_arms = Vec::new();
        let mut default = None;
        for arm in arms {
            let value = self.elab_expr(&arm.value, env);
            match &arm.pattern.node {
                ElabExprNode::Ident(name) if name == "default" => default = Some(value),
                _ => select_arms.push(EirSelectArm::new(self.elab_expr(&arm.pattern, env), value)),
            }
        }
        default.map_or_else(
            || EirExpr::unsupported("select expression has no default arm"),
            |default| EirExpr::select(mode, select_arms, default),
        )
    }

    pub(super) fn view_field_ref(
        &self,
        base: &EirExpr,
        ty: &MirTypeRef,
        field: &str,
    ) -> Option<EirExpr> {
        if ty.view_select().is_some() {
            return Some(EirExpr::ident(format!("{}_{}", base.fact_key(), field)));
        }
        None
    }

    pub(super) fn elab_actual_view_field(
        &self,
        actual: &ElabExpr,
        field: &str,
        env: &Env,
    ) -> EirExpr {
        if let ElabExprNode::Index { base, index } = &actual.node
            && let ElabExprNode::Ident(name) = &base.node
            && let Some(var) = env.vars.get(name)
            && let Some((_, elem)) = var.ty.array()
            && elem.view_select().is_some()
        {
            let port_expr = EirExpr::ident(format!("{}_{}", var.code.fact_key(), field));
            let width = self
                .view_field_type(env.owner, elem, field)
                .map(|ty| self.width_bound(env.owner, &ty))
                .unwrap_or_else(|| EirBound::new("1", EirExpr::Int(1)));
            if width.is_one() {
                return EirExpr::index(port_expr, self.elab_expr(index, env));
            }
            return EirExpr::indexed_part_select(port_expr, self.elab_expr(index, env), width);
        }
        if let ElabExprNode::Ident(name) = &actual.node
            && let Some(var) = env.vars.get(name)
            && let Some((_, elem)) = var.ty.array()
            && elem.view_select().is_some()
        {
            return EirExpr::ident(format!("{}_{}", var.code.fact_key(), field));
        }
        if let ElabExprNode::Ident(name) = &actual.node
            && let Some(var) = env.vars.get(name)
            && let Some(expr) = self.view_field_ref(&var.code, &var.ty, field)
        {
            return expr;
        }
        EirExpr::ident(format!(
            "{}_{}",
            self.elab_expr(actual, env).fact_key(),
            field
        ))
    }

    pub(super) fn view_field_type(
        &self,
        owner: Option<DefId>,
        ty: &MirTypeRef,
        field: &str,
    ) -> Option<MirTypeRef> {
        if let Some((len, elem)) = ty.array() {
            let field_ty = self.view_field_type(owner, elem, field)?;
            return Some(field_ty.with_array_len(len.clone(), ty.span()));
        }
        let (base, _) = ty.view_select()?;
        let interface = self.interface_for_type(owner, base)?;
        let field_decl = interface.fields.iter().find(|decl| decl.name == field)?;
        Some(self.subst_interface_field_type(owner, base, &field_decl.ty))
    }

    pub(super) fn elab_index_expr(&self, base: &ElabExpr, index: &ElabExpr, env: &Env) -> EirExpr {
        let base_code = self.elab_expr(base, env);
        if let ElabExprNode::Ident(name) = &base.node
            && let Some(var) = env.vars.get(name)
            && let Some((_, elem)) = var.ty.array()
        {
            let width = self.width_bound(env.owner, elem);
            if width.is_one() {
                return EirExpr::index(base_code, self.elab_expr(index, env));
            }
            return EirExpr::indexed_part_select(base_code, self.elab_expr(index, env), width);
        }
        EirExpr::index(base_code, self.elab_expr(index, env))
    }

    pub(super) fn bundle_field_ref(
        &self,
        owner: Option<DefId>,
        base: &EirExpr,
        ty: &MirTypeRef,
        field: &str,
    ) -> Option<EirExpr> {
        let bundle = self.bundle_for_type(owner, ty)?;
        let mut low = EirBound::new("0", EirExpr::Int(0));
        for decl in bundle.fields.iter().rev() {
            let width = self.width_bound(owner, &self.subst_bundle_field_type(owner, ty, &decl.ty));
            let high = EirBound::new(
                format!("({}) + ({}) - 1", low.source(), width.source()),
                EirExpr::binary(
                    EirBinaryOp::Sub,
                    EirExpr::binary(EirBinaryOp::Add, low.expr().clone(), width.expr().clone()),
                    EirExpr::Int(1),
                ),
            );
            if decl.name == field {
                return Some(EirExpr::slice(base.clone(), high, low));
            }
            low = EirBound::new(
                format!("({}) + ({})", low.source(), width.source()),
                EirExpr::binary(EirBinaryOp::Add, low.expr().clone(), width.expr().clone()),
            );
        }
        None
    }

    pub(super) fn elab_expr_name(&self, expr: &ElabExpr) -> Option<String> {
        let mut current = expr;
        loop {
            match &current.node {
                ElabExprNode::Ident(name) => return Some(name.clone()),
                ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }

    pub(super) fn elab_callee_root<'b>(&self, expr: &'b ElabExpr) -> Option<&'b ElabExpr> {
        let mut current = expr;
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

    pub(super) fn generic_actuals_for_elab(
        &self,
        def: DefId,
        callee: &ElabExpr,
        owner: Option<DefId>,
    ) -> Vec<EirParamBind> {
        let ElabExprNode::GenericApp { args, .. } = &callee.node else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(callable) = self.program.callable_by_def(def) {
            for (idx, generic) in callable.generics().iter().enumerate() {
                if self.is_domain_param(generic) {
                    continue;
                }
                if let Some(arg) = args.get(idx) {
                    if generic.kind.is_none() {
                        let arg = self.canonicalize_callsite_type(owner, arg);
                        out.push(EirParamBind::new(
                            format!("{}_WIDTH", generic.name),
                            self.width(owner.or(Some(def)), &arg),
                        ));
                    } else {
                        let arg = self.canonicalize_callsite_type(owner, arg);
                        out.push(EirParamBind::new(
                            &generic.name,
                            self.type_value(owner.or(Some(def)), &arg),
                        ));
                    }
                }
            }
        }
        out
    }

    pub(super) fn callable_result_for(&self, def: DefId) -> Option<&ElabSignatureResultBinding> {
        self.program.callable_by_def(def)?.result()
    }

    pub(super) fn callable_params_for_elab(
        &self,
        def: DefId,
        callable: &str,
        callee: &ElabExpr,
    ) -> Result<Vec<(String, MirTypeRef)>, CompileError> {
        let Some(callable_ref) = self.program.callable_by_def(def) else {
            return Err(CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier {
                    name: callable.to_string(),
                },
                callee.span(),
            ));
        };
        Ok(callable_ref
            .params()
            .iter()
            .map(|param| {
                let ty = param.ty.clone();
                (
                    param.name.clone(),
                    self.specialize_type_for_elab(&ty, def, callee),
                )
            })
            .collect())
    }

    pub(super) fn callable_result_type_from_elab(
        &self,
        callee: &ElabExpr,
        env: &Env,
    ) -> Option<MirTypeRef> {
        let def = self.callee_def_from_elab(callee, env)?;
        self.callable_result_type_for_elab_def(def, callee)
    }

    pub(super) fn callable_result_type_for_elab_def(
        &self,
        def: DefId,
        callee: &ElabExpr,
    ) -> Option<MirTypeRef> {
        let result = self.callable_result_for(def)?;
        Some(self.specialize_type_for_elab(&result.ty, def, callee))
    }

    pub(super) fn specialize_type_for_elab(
        &self,
        ty: &MirTypeRef,
        def: DefId,
        callee: &ElabExpr,
    ) -> MirTypeRef {
        let ElabExprNode::GenericApp { args, .. } = &callee.node else {
            return ty.clone();
        };
        let Some(callable) = self.program.callable_by_def(def) else {
            return ty.clone();
        };
        let mut replacements = HashMap::new();
        for (idx, generic) in callable.generics().iter().enumerate() {
            if let Some(arg) = args.get(idx) {
                replacements.insert(generic.name.clone(), arg.clone());
            }
        }
        self.subst_type_vars(ty, &replacements)
    }

    pub(super) fn callable_from_elab<'b>(
        &'b self,
        callee: &ElabExpr,
        env: &Env,
    ) -> Result<(DefId, String, &'b ElabCallable), CompileError> {
        let name = self.elab_expr_name(callee).ok_or_else(|| {
            CompileError::lowering_at(EirError::InstanceCalleeMustBeName, callee.span())
        })?;
        let def = env
            .owner
            .and_then(|owner| self.resolved_elab_callee_def(owner, callee))
            .ok_or_else(|| {
                CompileError::lowering_at(
                    ConstEvalError::UnknownElaborationIdentifier { name: name.clone() },
                    callee.span(),
                )
            })?;
        let callable = self.program.callable_by_def(def).ok_or_else(|| {
            CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier { name: name.clone() },
                callee.span(),
            )
        })?;
        let canonical_name = self.program.def_name(def).unwrap_or(&name).to_string();
        Ok((def, canonical_name, callable))
    }

    pub(super) fn callee_def_from_elab(&self, callee: &ElabExpr, env: &Env) -> Option<DefId> {
        let owner = env.owner?;
        self.resolved_elab_callee_def(owner, callee)
    }

    fn resolved_elab_callee_def(&self, owner: DefId, callee: &ElabExpr) -> Option<DefId> {
        let root = self.elab_callee_root(callee)?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, root) else {
            return None;
        };
        Some(def)
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
enum EirBuiltinIntrinsic {
    HighZ,
    Zero,
}

#[non_exhaustive]
struct EirBuiltinResolver<'a> {
    program: &'a ElabProgram,
    owner: Option<DefId>,
}

impl<'a> EirBuiltinResolver<'a> {
    fn new(program: &'a ElabProgram, owner: Option<DefId>) -> Self {
        Self { program, owner }
    }

    fn resolve_call_callee(&self, callee: &ElabExpr) -> Option<EirBuiltinIntrinsic> {
        let root = self.callee_root(callee)?;
        if self.has_user_resolution(root) {
            return None;
        }
        let ElabExprNode::Ident(name) = &root.node else {
            return None;
        };
        self.resolve_name(name)
    }

    fn has_user_resolution(&self, root: &ElabExpr) -> bool {
        let Some(owner) = self.owner else {
            return false;
        };
        matches!(
            self.program.expr_resolution(owner, root),
            Some(ElabResolution::Def(_) | ElabResolution::Local(_))
        )
    }

    fn resolve_name(&self, name: &str) -> Option<EirBuiltinIntrinsic> {
        match name {
            "z" => Some(EirBuiltinIntrinsic::HighZ),
            "zero" => Some(EirBuiltinIntrinsic::Zero),
            _ => None,
        }
    }

    fn callee_root<'b>(&self, callee: &'b ElabExpr) -> Option<&'b ElabExpr> {
        let mut current = callee;
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
