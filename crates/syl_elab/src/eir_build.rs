use crate::{
    CompileError,
    const_mir::ConstMirProgram,
    eir::{EirDesign, EirDesignAssembler, EirModule, EirParam},
    eir_connect::PortSpec,
    eir_expr::{EirBinaryOp, EirExpr, EirUnaryOp},
    eir_origin::{EirExpansion, EirOrigin},
    map_ir::MapIrProgram,
    mir::MirTypeRef,
    program::{
        ElabCallable, ElabCallableItem, ElabExpr, ElabExprNode, ElabExternModuleItem,
        ElabPortDirection, ElabProgram, ElabSignatureGenericParam,
    },
};
use std::collections::HashMap;
use syl_hir::DefId;
use syl_span::Span;

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct VarInfo {
    pub(crate) code: EirExpr,
    pub(crate) ty: MirTypeRef,
}

#[derive(Default, Clone)]
#[non_exhaustive]
pub(crate) struct Env {
    pub(crate) vars: HashMap<String, VarInfo>,
    vars_by_static_type: HashMap<String, Vec<String>>,
    pub(crate) type_replacements: HashMap<String, MirTypeRef>,
    pub(crate) expansion_stack: Vec<EirExpansion>,
    pub(crate) owner: Option<DefId>,
    prefix: Option<String>,
}

impl Env {
    pub(crate) fn with_owner(owner: DefId) -> Self {
        Self {
            owner: Some(owner),
            ..Self::default()
        }
    }

    pub(crate) fn with_prefix(prefix: impl Into<String>) -> Self {
        Self {
            vars: HashMap::new(),
            vars_by_static_type: HashMap::new(),
            type_replacements: HashMap::new(),
            expansion_stack: Vec::new(),
            owner: None,
            prefix: Some(prefix.into()),
        }
    }

    pub(crate) fn insert(&mut self, name: impl Into<String>, code: EirExpr, ty: MirTypeRef) {
        let name = name.into();
        let static_type = ty.type_name().map(ToOwned::to_owned);
        if let Some(previous) = self.vars.insert(name.clone(), VarInfo { code, ty })
            && let Some(static_type) = previous.ty.type_name().map(ToOwned::to_owned)
            && let Some(names) = self.vars_by_static_type.get_mut(&static_type)
        {
            names.retain(|existing| existing != &name);
            if names.is_empty() {
                self.vars_by_static_type.remove(&static_type);
            }
        }
        if let Some(static_type) = static_type {
            self.vars_by_static_type
                .entry(static_type)
                .or_default()
                .push(name);
        }
    }

    pub(crate) fn local_name(&self, name: &str) -> String {
        self.prefix
            .as_ref()
            .map(|prefix| format!("{prefix}_{name}"))
            .unwrap_or_else(|| name.to_string())
    }

    pub(crate) fn origin(&self, span: Span) -> EirOrigin {
        EirOrigin::new(span, self.expansion_stack.clone())
    }

    pub(crate) fn unique_label(&self, prefix: &str, span: Span) -> String {
        let mut label = prefix.to_string();
        for expansion in &self.expansion_stack {
            label.push('_');
            for ch in expansion.instance().chars() {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    label.push(ch);
                } else {
                    label.push('_');
                }
            }
        }
        label.push('_');
        label.push_str(&span.start.to_string());
        label
    }

    pub(crate) fn push_expansion(
        &mut self,
        callable: impl Into<String>,
        instance: impl Into<String>,
        span: Span,
    ) {
        self.expansion_stack
            .push(EirExpansion::new(callable, instance, span));
    }

    pub(crate) fn single_by_type(
        &self,
        type_name: &str,
        _emitter: &EirBuilder<'_>,
    ) -> Option<EirExpr> {
        let names = self.vars_by_static_type.get(type_name)?;
        if names.len() != 1 {
            return None;
        }
        self.vars.get(&names[0]).map(|var| var.code.clone())
    }

    pub(crate) fn clock_for_elab_reset_expr(
        &self,
        expr: &ElabExpr,
        emitter: &EirBuilder<'_>,
    ) -> Option<EirExpr> {
        let ElabExprNode::Ident(name) = &expr.node else {
            return None;
        };
        let reset = self.vars.get(name)?;
        if emitter.static_type_name(&reset.ty) != Some("Reset") {
            return None;
        }
        let reset_domain = emitter.first_type_arg(&reset.ty)?;
        let mut matches = self
            .vars_by_static_type
            .get("Clock")?
            .iter()
            .filter_map(|name| {
                self.vars.get(name).and_then(|var| {
                    (emitter.first_type_arg(&var.ty) == Some(reset_domain))
                        .then_some(var.code.clone())
                })
            });
        let first = matches.next()?;
        if matches.next().is_some() {
            None
        } else {
            Some(first)
        }
    }
}

#[non_exhaustive]
pub(crate) struct EirBuilder<'a> {
    pub(crate) const_mir: &'a ConstMirProgram,
    pub(crate) map_ir: &'a MapIrProgram,
    pub(crate) program: &'a ElabProgram,
}

impl<'a> EirBuilder<'a> {
    pub(crate) fn new(
        program: &'a ElabProgram,
        const_mir: &'a ConstMirProgram,
        map_ir: &'a MapIrProgram,
    ) -> Self {
        Self {
            const_mir,
            map_ir,
            program,
        }
    }

    pub(crate) fn build_design(&self) -> Result<EirDesign, CompileError> {
        let mut modules = Vec::new();
        for (owner, callable) in self.program.callables() {
            match callable {
                ElabCallable::Module(item) => {
                    modules.push(self.build_callable(*owner, item)?);
                }
                ElabCallable::Extern(item) => {
                    modules.push(self.build_extern(*owner, item)?);
                }
                ElabCallable::Cell(_) => {}
            }
        }
        EirDesignAssembler::assemble(modules)
    }

    fn build_callable(
        &self,
        owner: DefId,
        item: &ElabCallableItem,
    ) -> Result<EirModule, CompileError> {
        let mut env = Env {
            owner: Some(owner),
            ..Env::default()
        };
        for generic in &item.generics {
            self.insert_generic(&mut env, generic);
        }
        let mut ports = Vec::new();
        for param in &item.params {
            let param_ty = param.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                PortSpec {
                    name: &param.name,
                    dir: param.direction,
                    ty: &param_ty,
                    span: param.span,
                },
            )?;
        }
        if let Some(result) = &item.result {
            let result_ty = result.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                PortSpec {
                    name: &result.name,
                    dir: ElabPortDirection::Out,
                    ty: &result_ty,
                    span: result.span,
                },
            )?;
        }

        let params = self.generic_params(&env, &item.generics);
        let items = self.emit_body(&item.body, &mut env)?;
        Ok(EirModule::new(&item.name, params, ports, items))
    }

    fn build_extern(
        &self,
        owner: DefId,
        item: &ElabExternModuleItem,
    ) -> Result<EirModule, CompileError> {
        let mut env = Env {
            owner: Some(owner),
            ..Env::default()
        };
        for generic in &item.generics {
            self.insert_generic(&mut env, generic);
        }
        let mut ports = Vec::new();
        for param in &item.params {
            let param_ty = param.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                PortSpec {
                    name: &param.name,
                    dir: param.direction,
                    ty: &param_ty,
                    span: param.span,
                },
            )?;
        }
        if let Some(result) = &item.result {
            let result_ty = result.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                PortSpec {
                    name: &result.name,
                    dir: ElabPortDirection::Out,
                    ty: &result_ty,
                    span: result.span,
                },
            )?;
        }
        let params = self.generic_params(&env, &item.generics);
        Ok(EirModule::new_extern(&item.name, params, ports))
    }

    fn generic_params(&self, env: &Env, params: &[ElabSignatureGenericParam]) -> Vec<EirParam> {
        let mut out = Vec::new();
        for param in params {
            if self.is_domain_param(param) {
                continue;
            }
            if param.kind.is_none() {
                out.push(EirParam::new(format!("{}_WIDTH", param.name), "1"));
                continue;
            }
            let default = param
                .default
                .as_ref()
                .map(|expr| self.elab_expr(expr, env))
                .map(|expr| expr.fact_key())
                .unwrap_or_else(|| "1".to_string());
            out.push(EirParam::new(&param.name, default));
        }
        out
    }

    pub(crate) fn is_domain_param(&self, param: &ElabSignatureGenericParam) -> bool {
        param
            .kind
            .as_ref()
            .is_some_and(|kind| kind.path_name().is_some_and(|name| name == "Domain"))
    }

    pub(crate) fn insert_generic(&self, env: &mut Env, param: &ElabSignatureGenericParam) {
        if let Some(kind) = &param.kind
            && matches!(self.static_type_name(kind), Some("Nat" | "Bool"))
        {
            env.insert(&param.name, EirExpr::ident(&param.name), kind.clone());
        }
    }

    pub(crate) fn static_type_name<'b>(&self, ty: &'b MirTypeRef) -> Option<&'b str> {
        ty.type_name()
    }

    pub(crate) fn first_type_arg<'b>(&self, ty: &'b MirTypeRef) -> Option<&'b MirTypeRef> {
        ty.args()?.first()
    }

    pub(crate) fn elab_expr(&self, expr: &ElabExpr, env: &Env) -> EirExpr {
        match &expr.node {
            ElabExprNode::Ident(name) => {
                if let Some(var) = env.vars.get(name) {
                    var.code.clone()
                } else if let Some(item) = self.const_for_name(env.owner, name) {
                    self.elab_expr(&item.value, env)
                } else if let Some(value) = self.program.enum_variant_value_by_name(env.owner, name)
                {
                    EirExpr::Int(value)
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
            ElabExprNode::Inst { .. } => EirExpr::unsupported("inst is not a value expression"),
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

    pub(crate) fn sanitize(&self, name: &str) -> String {
        name.chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect()
    }

    pub(crate) fn enum_width(&self, count: usize) -> usize {
        let mut width = 1usize;
        let mut capacity = 2usize;
        while capacity < count {
            capacity *= 2;
            width += 1;
        }
        width
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syl_span::Span;

    fn named_type(name: &str) -> MirTypeRef {
        MirTypeRef::path_type(vec![name.to_string()], Span::new(0, 0))
    }

    #[test]
    fn insert_indexes_vars_by_static_type() {
        let mut env = Env::default();
        env.insert("clock_a", EirExpr::ident("clock_a"), named_type("Clock"));
        env.insert("clock_b", EirExpr::ident("clock_b"), named_type("Clock"));
        env.insert("reset_a", EirExpr::ident("reset_a"), named_type("Reset"));

        assert_eq!(
            env.vars_by_static_type.get("Clock").cloned(),
            Some(vec!["clock_a".to_string(), "clock_b".to_string()])
        );
        assert_eq!(
            env.vars_by_static_type.get("Reset").cloned(),
            Some(vec!["reset_a".to_string()])
        );
    }

    #[test]
    fn insert_replaces_previous_static_type_index_entry() {
        let mut env = Env::default();
        env.insert("value", EirExpr::ident("value"), named_type("Clock"));
        env.insert("value", EirExpr::ident("value"), named_type("Reset"));

        assert_eq!(env.vars_by_static_type.get("Clock"), None);
        assert_eq!(
            env.vars_by_static_type.get("Reset").cloned(),
            Some(vec!["value".to_string()])
        );
    }
}
