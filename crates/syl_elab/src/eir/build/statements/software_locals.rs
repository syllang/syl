use crate::{
    CompileError, EirError,
    eir::EirExpr,
    eir::build::VarInfo,
    mir::MirTypeRef,
    program::{ElabExpr, ElabExprNode},
    tir::TirType,
};
use std::collections::BTreeSet;
use syl_sema::tir::TirGenericArg;
use syl_span::Span;

use super::{EirBuilder, Env, NumberingValue};

pub(super) struct BindVarRequest<'a> {
    pub(super) id: Option<syl_hir::LocalId>,
    pub(super) name: &'a str,
    pub(super) ty: Option<&'a MirTypeRef>,
    pub(super) value: Option<&'a ElabExpr>,
    pub(super) span: Span,
}

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn bind_var(
        &self,
        binding: BindVarRequest<'_>,
        env: &mut Env,
    ) -> Result<(), CompileError> {
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
        let numbering_value = self.initial_software_local_numbering_value(binding.value, env);
        env.insert_software_local_with_numbering(binding.name, code, ty, numbering_value);
        self.sync_software_local_fields(binding.name, binding.value, env);
        self.rebuild_software_root_binding(binding.name, env);
        Ok(())
    }

    pub(super) fn emit_assign(
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
                let numbering_value =
                    self.assigned_software_local_numbering_value(name, value, env);
                env.insert_software_local_with_numbering(
                    name,
                    self.elab_expr(value, env),
                    ty,
                    numbering_value,
                );
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
                env.insert_software_local_with_numbering(
                    field_name,
                    self.elab_expr(value, env),
                    field_ty,
                    None,
                );
                self.rebuild_software_root_binding(root, env);
                Ok(())
            }
            _ => Err(CompileError::lowering_at(
                EirError::InvalidElaborationExpression,
                span,
            )),
        }
    }

    pub(super) fn sync_visible_software_locals(&self, source: &Env, target: &mut Env) {
        let visible = target.vars.keys().cloned().collect::<Vec<_>>();
        for (name, var) in &source.vars {
            let root_visible = name
                .split_once('.')
                .and_then(|(root, _)| target.var(root))
                .is_some_and(|var| var.software_local);
            if (visible.iter().any(|existing| existing == name) || root_visible)
                && var.software_local
            {
                target.insert_software_local_with_numbering(
                    name.clone(),
                    var.code.clone(),
                    var.ty.clone(),
                    var.numbering_value,
                );
            }
        }
    }

    pub(super) fn merge_visible_software_locals_between_branches(
        &self,
        cond: &EirExpr,
        then_env: &Env,
        else_env: &Env,
        target: &mut Env,
    ) {
        for name in self.visible_software_local_names(target, &[then_env, else_env]) {
            let Some(current) = target.var(&name).cloned() else {
                continue;
            };
            let then_var = then_env.var(&name);
            let else_var = else_env.var(&name);
            let merged_numbering =
                self.merge_software_local_branch_numbering(&current, then_var, else_var);
            let merged = self.symbolic_branch_merge_value(cond, &current, then_var, else_var);
            target.insert_software_local_with_numbering(name, merged.0, merged.1, merged_numbering);
        }
        self.rebuild_visible_software_roots(target);
    }

    pub(super) fn merge_visible_software_locals_after_conditional_branch(
        &self,
        cond: &EirExpr,
        source: &Env,
        target: &mut Env,
    ) {
        for name in self.visible_software_local_names(target, &[source]) {
            let Some(current) = target.var(&name).cloned() else {
                continue;
            };
            let merged =
                self.symbolic_branch_merge_value(cond, &current, source.var(&name), Some(&current));
            let merged_numbering = self.merge_software_local_branch_numbering(
                &current,
                source.var(&name),
                Some(&current),
            );
            target.insert_software_local_with_numbering(name, merged.0, merged.1, merged_numbering);
        }
        self.rebuild_visible_software_roots(target);
    }

    pub(super) fn merge_visible_software_locals_after_loop(&self, source: &Env, target: &mut Env) {
        for name in self.visible_software_local_names(target, &[source]) {
            let Some(current) = target.var(&name) else {
                continue;
            };
            let merged_numbering =
                self.merge_software_local_loop_numbering(current, source.var(&name));
            let merged = match source.var(&name) {
                Some(updated) if updated.ty == current.ty && updated.code == current.code => {
                    (updated.code.clone(), updated.ty.clone())
                }
                Some(_) => (self.unknown_software_local_expr(&name), current.ty.clone()),
                None => (current.code.clone(), current.ty.clone()),
            };
            target.insert_software_local_with_numbering(name, merged.0, merged.1, merged_numbering);
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

    pub(super) fn initial_software_local_numbering_value(
        &self,
        value: Option<&ElabExpr>,
        env: &Env,
    ) -> Option<NumberingValue> {
        let expr = value?;
        match &expr.node {
            ElabExprNode::Int(value) => Some(NumberingValue::Counter(*value)),
            ElabExprNode::Group(inner) => {
                self.initial_software_local_numbering_value(Some(inner), env)
            }
            _ => None,
        }
    }

    pub(super) fn assigned_software_local_numbering_value(
        &self,
        target_name: &str,
        value: &ElabExpr,
        env: &Env,
    ) -> Option<NumberingValue> {
        match &value.node {
            ElabExprNode::Group(inner) => {
                self.assigned_software_local_numbering_value(target_name, inner, env)
            }
            ElabExprNode::Binary { op, left, right }
                if matches!(op, crate::mir::MirBinaryOp::Add) =>
            {
                self.counter_increment_value(target_name, left, right, env)
                    .or_else(|| self.counter_increment_value(target_name, right, left, env))
            }
            _ => None,
        }
    }

    pub(super) fn merge_software_local_branch_numbering(
        &self,
        current: &VarInfo,
        then_var: Option<&VarInfo>,
        else_var: Option<&VarInfo>,
    ) -> Option<NumberingValue> {
        match (then_var, else_var) {
            (Some(then_var), Some(else_var)) => {
                self.merge_numbering_values(then_var.numbering_value, else_var.numbering_value)
            }
            _ => current.numbering_value,
        }
    }

    pub(super) fn merge_software_local_loop_numbering(
        &self,
        current: &VarInfo,
        updated: Option<&VarInfo>,
    ) -> Option<NumberingValue> {
        match updated {
            Some(updated) => {
                self.merge_numbering_values(current.numbering_value, updated.numbering_value)
            }
            None => current.numbering_value,
        }
    }

    pub(super) fn merge_numbering_values(
        &self,
        lhs: Option<NumberingValue>,
        rhs: Option<NumberingValue>,
    ) -> Option<NumberingValue> {
        match (lhs, rhs) {
            (Some(NumberingValue::Counter(lhs)), Some(NumberingValue::Counter(rhs))) => {
                Some(NumberingValue::Counter(lhs.max(rhs)))
            }
            _ => None,
        }
    }

    fn symbolic_branch_merge_value(
        &self,
        cond: &EirExpr,
        current: &VarInfo,
        when_true: Option<&VarInfo>,
        when_false: Option<&VarInfo>,
    ) -> (EirExpr, MirTypeRef) {
        let (then_code, then_ty) = when_true
            .map(|var| (var.code.clone(), var.ty.clone()))
            .unwrap_or_else(|| (current.code.clone(), current.ty.clone()));
        let (else_code, else_ty) = when_false
            .map(|var| (var.code.clone(), var.ty.clone()))
            .unwrap_or_else(|| (current.code.clone(), current.ty.clone()));
        if then_ty == else_ty && then_code == else_code {
            return (then_code, then_ty);
        }
        (
            EirExpr::mux(cond.clone(), then_code, else_code),
            current.ty.clone(),
        )
    }

    pub(super) fn counter_increment_value(
        &self,
        target_name: &str,
        counter_expr: &ElabExpr,
        delta_expr: &ElabExpr,
        env: &Env,
    ) -> Option<NumberingValue> {
        let ElabExprNode::Ident(source) = &counter_expr.node else {
            return None;
        };
        if source != target_name {
            return None;
        }
        let base = env.var(source)?.numbering_value?;
        let delta = Self::local_const_nat(delta_expr, env)?;
        Some(NumberingValue::Counter(base.value() + delta))
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
                        env.insert_software_local_with_numbering(
                            format!("{name}.{}", field.name),
                            self.elab_expr(&field.value, env),
                            field_ty,
                            None,
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
                    var.numbering_value,
                )
            })
            .collect::<Vec<_>>();
        for (name, code, ty, numbering_value) in fields {
            env.insert_software_local_with_numbering(name, code, ty, numbering_value);
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
        env.insert_software_local_with_numbering(root, code, root_var.ty, root_var.numbering_value);
    }

    fn rebuild_visible_software_roots(&self, env: &mut Env) {
        let roots = env
            .vars
            .iter()
            .filter(|(_, var)| var.software_local)
            .filter_map(|(name, _)| name.split_once('.').map(|(root, _)| root.to_string()))
            .collect::<BTreeSet<_>>();
        for root in roots {
            self.rebuild_software_root_binding(&root, env);
        }
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

    pub(super) fn local_const_bool(expr: &ElabExpr, env: &Env) -> Option<bool> {
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
