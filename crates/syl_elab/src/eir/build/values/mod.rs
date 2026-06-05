mod builtins;
mod calls;

use crate::{
    CompileError,
    const_eval::{ConstEvalEnv, ConstKind, ConstValue},
    eir::{EirBinaryOp, EirBound, EirExpr, EirSelectArm, EirSelectMode, EirUnaryOp},
    program::{ElabDefKind, ElabExpr, ElabExprNode, ElabMatchArm, ElabNamedExpr, ElabSelectArm},
};
use std::collections::{BTreeSet, HashMap};
use syl_hir::DefId;
use syl_sema::ir::const_mir::{ConstStructFieldValue, ConstStructKind, ConstStructValue};

use super::{EirBuilder, Env, VarInfo};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn elab_const_value(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<ConstValue, CompileError> {
        self.const_elaborator
            .elab_value(self.program, expr, &mut self.const_eval_env(env))
    }

    pub(super) fn elab_const_bool(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<Option<bool>, CompileError> {
        self.const_elaborator
            .elab_bool(self.program, expr, &mut self.const_eval_env(env))
    }

    pub(super) fn elab_require_const_nat(
        &self,
        expr: &ElabExpr,
        env: &Env,
        context: &str,
    ) -> Result<ConstValue, CompileError> {
        self.const_elaborator.require_elab_nat(
            self.program,
            expr,
            &mut self.const_eval_env(env),
            context,
        )
    }

    pub(in crate::eir::build) fn const_eval_env(&self, env: &Env) -> ConstEvalEnv {
        let mut out = ConstEvalEnv::with_owner(env.owner);
        for (name, var) in &env.vars {
            if let Some(value) = self.const_value_for_var(name, var, env) {
                out.bind(name.clone(), value);
            }
        }
        out
    }

    fn const_value_for_var(&self, name: &str, var: &VarInfo, env: &Env) -> Option<ConstValue> {
        let ty = self.canonicalize_const_eval_type(env.owner, &var.ty);
        let kind = self.const_elaborator.kind_for_type(&ty);
        match &var.code {
            EirExpr::Int(value) => Some(ConstValue::Nat(*value)),
            EirExpr::Bool(value) => Some(ConstValue::Bool(*value)),
            _ => {
                if var.software_local
                    && let Some(ConstKind::Struct(kind)) = kind
                    && let Some(value) = self.software_local_struct_value(name, kind, env)
                {
                    return Some(value);
                }
                kind.map(ConstValue::Unknown)
            }
        }
    }

    fn software_local_struct_value(
        &self,
        root: &str,
        kind: ConstStructKind,
        env: &Env,
    ) -> Option<ConstValue> {
        let prefix = format!("{root}.");
        let field_names = env
            .vars
            .iter()
            .filter(|(name, var)| var.software_local && name.starts_with(&prefix))
            .filter_map(|(name, _)| name[prefix.len()..].split('.').next().map(str::to_string))
            .collect::<BTreeSet<_>>();
        if field_names.is_empty() {
            return None;
        }
        let fields = field_names
            .into_iter()
            .map(|field_name| {
                let field_key = format!("{root}.{field_name}");
                let field_var = env.var(&field_key)?;
                let field_value = self.const_value_for_var(&field_key, field_var, env)?;
                Some(ConstStructFieldValue::new(field_name, field_value))
            })
            .collect::<Option<Vec<_>>>()?;
        Some(ConstValue::Struct(ConstStructValue::new(kind, fields)))
    }

    fn canonicalize_const_eval_type(
        &self,
        owner: Option<DefId>,
        ty: &crate::mir::MirTypeRef,
    ) -> crate::mir::MirTypeRef {
        let Some(owner) = owner else {
            return ty.clone();
        };
        if let Some(path) = ty.path() {
            return self.canonicalize_const_eval_path(owner, ty, path);
        }
        if let Some((len, elem)) = ty.array() {
            return crate::mir::MirTypeRef::array_type(
                len.clone(),
                self.canonicalize_const_eval_type(Some(owner), elem),
                ty.span(),
            );
        }
        if let Some((base, view)) = ty.view_select() {
            return crate::mir::MirTypeRef::view_select_type(
                self.canonicalize_const_eval_type(Some(owner), base),
                view.to_string(),
                ty.span(),
            );
        }
        if let Some(base) = ty.generic_base() {
            let args = ty
                .args()
                .unwrap_or_default()
                .iter()
                .map(|arg| self.canonicalize_const_eval_type(Some(owner), arg))
                .collect();
            return crate::mir::MirTypeRef::generic_type(
                self.canonicalize_const_eval_type(Some(owner), base),
                args,
                ty.span(),
            );
        }
        ty.clone()
    }

    fn canonicalize_const_eval_path(
        &self,
        owner: DefId,
        ty: &crate::mir::MirTypeRef,
        path: &[String],
    ) -> crate::mir::MirTypeRef {
        let def = if path.len() == 1 {
            self.program.resolve_def_id(owner, &path[0])
        } else {
            self.program.canonical_def_id(path)
        };
        let Some(def) = def else {
            return ty.clone();
        };
        let Some(canonical_path) = self.program.canonical_path(def) else {
            return ty.clone();
        };
        crate::mir::MirTypeRef::path_type(canonical_path.segments().to_vec(), ty.span())
    }

    pub(in crate::eir::build) fn summary_value_for_var(
        &self,
        name: &str,
        var: &VarInfo,
        env: &Env,
    ) -> Option<ConstValue> {
        var.summary_value
            .clone()
            .or_else(|| self.const_value_for_var(name, var, env))
    }

    pub(in crate::eir::build) fn summary_const_eval_env(&self, env: &Env) -> ConstEvalEnv {
        let mut out = ConstEvalEnv::with_owner(env.owner);
        for (name, var) in &env.vars {
            if let Some(value) = self.summary_value_for_var(name, var, env) {
                out.bind(name.clone(), value);
            }
        }
        out
    }

    pub(in crate::eir::build) fn elab_summary_const_value(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<ConstValue, CompileError> {
        self.const_elaborator
            .elab_value(self.program, expr, &mut self.summary_const_eval_env(env))
    }

    pub(super) fn elab_expr(&self, expr: &ElabExpr, env: &Env) -> EirExpr {
        match &expr.node {
            ElabExprNode::Ident(name) => {
                if let Some(var) = env.vars.get(name) {
                    var.code.clone()
                } else if let Some(item) = self.const_for_name(env.owner, name) {
                    self.elab_expr(&item.value, env)
                } else {
                    EirExpr::ident(name)
                }
            }
            ElabExprNode::Int(value) => EirExpr::Int(*value),
            ElabExprNode::Bool(value) => EirExpr::Bool(*value),
            ElabExprNode::Str(value) => EirExpr::Str(value.clone()),
            ElabExprNode::Unary { op, expr } => {
                let op = match op {
                    crate::mir::MirUnaryOp::Neg => EirUnaryOp::Neg,
                    crate::mir::MirUnaryOp::Not | crate::mir::MirUnaryOp::NotWord => {
                        EirUnaryOp::Not
                    }
                    crate::mir::MirUnaryOp::Unsupported => {
                        return EirExpr::unsupported("unsupported unary operator");
                    }
                    _ => return EirExpr::unsupported("unsupported unary operator"),
                };
                EirExpr::unary(op, self.elab_expr(expr, env))
            }
            ElabExprNode::Binary { op, left, right } => {
                let op = match op {
                    crate::mir::MirBinaryOp::OrOr => EirBinaryOp::OrOr,
                    crate::mir::MirBinaryOp::AndAnd => EirBinaryOp::AndAnd,
                    crate::mir::MirBinaryOp::Eq => EirBinaryOp::Eq,
                    crate::mir::MirBinaryOp::NotEq => EirBinaryOp::NotEq,
                    crate::mir::MirBinaryOp::Lt => EirBinaryOp::Lt,
                    crate::mir::MirBinaryOp::LtEq => EirBinaryOp::LtEq,
                    crate::mir::MirBinaryOp::Gt => EirBinaryOp::Gt,
                    crate::mir::MirBinaryOp::GtEq => EirBinaryOp::GtEq,
                    crate::mir::MirBinaryOp::Add => EirBinaryOp::Add,
                    crate::mir::MirBinaryOp::Sub => EirBinaryOp::Sub,
                    crate::mir::MirBinaryOp::Mul => EirBinaryOp::Mul,
                    crate::mir::MirBinaryOp::Div => EirBinaryOp::Div,
                    crate::mir::MirBinaryOp::Rem => EirBinaryOp::Rem,
                    crate::mir::MirBinaryOp::Shl => EirBinaryOp::Shl,
                    crate::mir::MirBinaryOp::BitAnd => EirBinaryOp::BitAnd,
                    crate::mir::MirBinaryOp::BitOr => EirBinaryOp::BitOr,
                    crate::mir::MirBinaryOp::BitXor => EirBinaryOp::BitXor,
                    _ => return EirExpr::unsupported("unsupported binary operator"),
                };
                EirExpr::binary(op, self.elab_expr(left, env), self.elab_expr(right, env))
            }
            ElabExprNode::Field { base, field } => self.elab_field_expr(base, field, env),
            ElabExprNode::Index { base, index } => self.elab_index_expr(base, index, env),
            ElabExprNode::Group(expr) => self.elab_expr(expr, env),
            ElabExprNode::GenericApp { callee, .. } => self.elab_expr(callee, env),
            ElabExprNode::Call { callee, args } => self.elab_call_expr(callee, args, env),
            ElabExprNode::Place { .. } => EirExpr::unsupported("place is not a value expression"),
            ElabExprNode::For { .. } => {
                EirExpr::unsupported("for placement is not a value expression")
            }
            ElabExprNode::Aggregate { ty, fields } => self.elab_aggregate_expr(ty, fields, env),
            ElabExprNode::Match { expr, arms } => self.elab_match_expr(expr, arms, env),
            ElabExprNode::Select { mode, arms } => self.elab_select_expr(*mode, arms, env),
            ElabExprNode::Block(block) => {
                let _has_tail = block.tail.is_some();
                EirExpr::unsupported("unsupported hardware value expression")
            }
            ElabExprNode::CompileError { .. }
            | ElabExprNode::Range { .. }
            | ElabExprNode::Unsupported => {
                EirExpr::unsupported("unsupported hardware value expression")
            }
        }
    }

    fn elab_field_expr(&self, base: &ElabExpr, field: &str, env: &Env) -> EirExpr {
        if let Some(owner) = env.owner
            && let Some(value) = self.program.enum_variant_field_value(owner, base, field)
        {
            return EirExpr::Int(value);
        }
        if self.elab_is_indexed_view_field(base, env) {
            return self.elab_actual_view_field(base, field, env);
        }
        let base_code = self.elab_expr(base, env);
        let base_key = base_code.fact_key();
        if let Some(var) = env.vars.get(&base_key) {
            if let Some(expr) = self.view_field_ref(&var.code, &var.ty, field) {
                return expr;
            }
            if let Some(expr) = self.bundle_field_ref(env.owner, &var.code, &var.ty, field) {
                return expr;
            }
        }
        if let ElabExprNode::Ident(name) = &base.node {
            if let Some(var) = env.vars.get(name) {
                if let Some(expr) = self.view_field_ref(&var.code, &var.ty, field) {
                    return expr;
                }
                if let Some(expr) = self.bundle_field_ref(env.owner, &var.code, &var.ty, field) {
                    return expr;
                }
            }
            if let Some(var) = env.vars.get(&format!("{name}.{field}")) {
                return var.code.clone();
            }
        }
        if let Some(var) = env.vars.get(&format!("{base_key}.{field}")) {
            return var.code.clone();
        }
        EirExpr::ident(format!("{base_key}_{field}"))
    }

    fn elab_is_indexed_view_field(&self, base: &ElabExpr, env: &Env) -> bool {
        let ElabExprNode::Index { base, .. } = &base.node else {
            return false;
        };
        let ElabExprNode::Ident(name) = &base.node else {
            return false;
        };
        env.vars.get(name).is_some_and(|var| {
            var.ty
                .array()
                .is_some_and(|(_, elem)| elem.view_select().is_some())
        })
    }

    pub(super) fn elab_aggregate_expr(
        &self,
        ty: &crate::mir::MirTypeRef,
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
        let target_enum_def = self.match_target_enum_def(target, env);
        let mut fallback = None;
        for arm in arms.iter().rev() {
            let value = self.elab_expr(&arm.value, env);
            match &arm.pattern {
                crate::mir::MirPattern::Wildcard(_) => fallback = Some(value),
                crate::mir::MirPattern::Ident(name, _) if name == "default" => {
                    fallback = Some(value)
                }
                pattern => {
                    let cond =
                        self.elab_match_pattern_condition(target, pattern, env, target_enum_def);
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
        pattern: &crate::mir::MirPattern,
        env: &Env,
        target_enum_def: Option<DefId>,
    ) -> EirExpr {
        EirExpr::binary(
            EirBinaryOp::Eq,
            self.elab_expr(target, env),
            self.elab_match_pattern_value(pattern, env, target_enum_def),
        )
    }

    fn elab_match_pattern_value(
        &self,
        pattern: &crate::mir::MirPattern,
        env: &Env,
        target_enum_def: Option<DefId>,
    ) -> EirExpr {
        match pattern {
            crate::mir::MirPattern::Path(path, _) => path
                .last()
                .map(|variant| {
                    if path.len() == 1 {
                        target_enum_def
                            .and_then(|enum_def| {
                                self.program.enum_variant_value_for_def(enum_def, variant)
                            })
                            .map(EirExpr::Int)
                            .unwrap_or_else(|| EirExpr::ident(variant))
                    } else {
                        env.owner
                            .and_then(|owner| self.program.enum_variant_value(owner, path))
                            .map(EirExpr::Int)
                            .unwrap_or_else(|| EirExpr::ident(variant))
                    }
                })
                .unwrap_or_else(|| EirExpr::unsupported("empty match path pattern")),
            crate::mir::MirPattern::Ident(name, _) => env
                .owner
                .and_then(|owner| self.program.enum_variant_value_by_name(Some(owner), name))
                .map(EirExpr::Int)
                .unwrap_or_else(|| EirExpr::ident(name)),
            crate::mir::MirPattern::Int(value, _) => EirExpr::Int(*value),
            crate::mir::MirPattern::Bool(value, _) => EirExpr::Bool(*value),
            crate::mir::MirPattern::Wildcard(_) => {
                EirExpr::unsupported("wildcard is not a condition")
            }
            crate::mir::MirPattern::Unsupported(_) => {
                EirExpr::unsupported("unsupported match pattern")
            }
            _ => EirExpr::unsupported("unsupported match pattern"),
        }
    }

    fn match_target_enum_def(&self, target: &ElabExpr, env: &Env) -> Option<DefId> {
        let owner = env.owner?;
        let def = self.program.expr_type(owner, target)?.definition()?;
        (self.program.def_kind(def) == Some(ElabDefKind::Enum)).then_some(def)
    }

    pub(super) fn elab_select_expr(
        &self,
        mode: crate::mir::MirSelectMode,
        arms: &[ElabSelectArm],
        env: &Env,
    ) -> EirExpr {
        let mode = match mode {
            crate::mir::MirSelectMode::Priority => EirSelectMode::Priority,
            crate::mir::MirSelectMode::Unique => EirSelectMode::Unique,
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
        ty: &crate::mir::MirTypeRef,
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
        ty: &crate::mir::MirTypeRef,
        field: &str,
    ) -> Option<crate::mir::MirTypeRef> {
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
        ty: &crate::mir::MirTypeRef,
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
}
