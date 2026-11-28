use crate::{
    eir::{EirExpansion, EirExpr, EirOrigin},
    mir::MirTypeRef,
    program::{ElabExpr, ElabExprNode},
};
use std::collections::HashMap;
use syl_hir::DefId;
use syl_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum NumberingValue {
    Counter(u64),
}

impl NumberingValue {
    pub(crate) fn value(self) -> u64 {
        match self {
            Self::Counter(value) => value,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct VarInfo {
    pub(crate) is_memory: bool,
    pub(crate) code: EirExpr,
    pub(crate) ty: MirTypeRef,
    pub(crate) software_local: bool,
    pub(crate) numbering_value: Option<NumberingValue>,
}

#[derive(Default, Clone)]
#[non_exhaustive]
pub(crate) struct Env {
    pub(crate) vars: HashMap<String, VarInfo>,
    pub(crate) type_replacements: HashMap<String, MirTypeRef>,
    control_aliases: HashMap<String, String>,
    vars_by_static_type: HashMap<String, Vec<String>>,
    pub(crate) expansion_stack: Vec<EirExpansion>,
    pub(crate) owner: Option<DefId>,
    pub(crate) prefix: Option<String>,
    pub(crate) expr_place_prefix: Option<String>,
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
            prefix: Some(prefix.into()),
            ..Self::default()
        }
    }

    pub(crate) fn insert(&mut self, name: impl Into<String>, code: EirExpr, ty: MirTypeRef) {
        let numbering_value = Self::default_numbering_value(&code);
        self.insert_var(
            name,
            VarInfo {
                is_memory: false,
                code,
                ty,
                software_local: false,
                numbering_value,
            },
        );
    }

    pub(crate) fn insert_with_numbering(
        &mut self,
        name: impl Into<String>,
        code: EirExpr,
        ty: MirTypeRef,
        numbering_value: Option<NumberingValue>,
    ) {
        self.insert_var(
            name,
            VarInfo {
                is_memory: false,
                code,
                ty,
                software_local: false,
                numbering_value,
            },
        );
    }

    pub(crate) fn insert_software_local(
        &mut self,
        name: impl Into<String>,
        code: EirExpr,
        ty: MirTypeRef,
    ) {
        let numbering_value = Self::default_numbering_value(&code);
        self.insert_var(
            name,
            VarInfo {
                is_memory: false,
                code,
                ty,
                software_local: true,
                numbering_value,
            },
        );
    }

    pub(crate) fn insert_software_local_with_numbering(
        &mut self,
        name: impl Into<String>,
        code: EirExpr,
        ty: MirTypeRef,
        numbering_value: Option<NumberingValue>,
    ) {
        self.insert_var(
            name,
            VarInfo {
                is_memory: false,
                code,
                ty,
                software_local: true,
                numbering_value,
            },
        );
    }

    fn insert_var(&mut self, name: impl Into<String>, var: VarInfo) {
        let name = name.into();
        let static_type = var.ty.type_name().map(ToOwned::to_owned);
        if let Some(previous) = self.vars.insert(name.clone(), var)
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

    fn default_numbering_value(code: &EirExpr) -> Option<NumberingValue> {
        match code {
            EirExpr::Int(value) => Some(NumberingValue::Counter(*value)),
            _ => None,
        }
    }

    pub(crate) fn var(&self, name: &str) -> Option<&VarInfo> {
        self.vars.get(name)
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
        source_name: String,
    ) {
        self.expansion_stack
            .push(EirExpansion::new(callable, instance, span).with_source_name(source_name));
    }

    /// Record only direct wires between controls. A gate or an inverted reset
    /// produces a distinct signal, even when all its inputs have the same domain.
    pub(crate) fn record_control_alias<C: crate::const_eval::ConstValueElaborator + ?Sized>(
        &mut self,
        target: &str,
        value: &ElabExpr,
        builder: &super::EirBuilder<'_, C>,
    ) {
        let mut value = value;
        while let ElabExprNode::Group(inner) = &value.node {
            value = inner;
        }
        let ElabExprNode::Ident(source) = &value.node else {
            return;
        };
        let (Some(target), Some(source)) = (self.var(target), self.var(source)) else {
            return;
        };
        let kind = target.ty.type_name();
        if !matches!(kind, Some("Clock" | "Reset")) {
            return;
        }
        let (EirExpr::Ident(lhs), EirExpr::Ident(rhs)) = (&target.code, &source.code) else {
            return;
        };
        let domain = builder.port_domain(&target.ty);
        if lhs != rhs
            && (builder.port_domain(&source.ty) == domain
                || self
                    .vars
                    .values()
                    .any(|v| v.code == source.code && builder.port_domain(&v.ty) == domain))
        {
            self.control_aliases.insert(lhs.clone(), rhs.clone());
        }
    }

    pub(crate) fn control_root(&self, code: &EirExpr) -> EirExpr {
        let EirExpr::Ident(start) = code else {
            return code.clone();
        };
        let mut name = start;
        // A cyclic continuous drive is invalid. Leave it to driver validation.
        for _ in 0..=self.control_aliases.len() {
            let Some(parent) = self.control_aliases.get(name) else {
                return EirExpr::ident(name);
            };
            name = parent;
        }
        code.clone()
    }

    fn unique_control(
        &self,
        kind: &str,
        matches: impl Fn(&VarInfo) -> bool,
    ) -> Result<Option<EirExpr>, (String, String)> {
        let mut candidates = self
            .vars_by_static_type
            .get(kind)
            .into_iter()
            .flatten()
            .filter_map(|name| self.var(name))
            .filter(|v| matches(v))
            .map(|v| self.control_root(&v.code));
        let first = candidates.next();
        if let Some(root) = &first
            && let Some(other) = candidates.find(|v| v != root)
        {
            return Err((root.fact_key(), other.fact_key()));
        }
        Ok(first)
    }

    pub(crate) fn single_by_type(&self, kind: &str) -> Option<EirExpr> {
        self.unique_control(kind, |_| true).ok().flatten()
    }

    pub(crate) fn clock_for_elab_reset_expr<C>(
        &self,
        expr: &ElabExpr,
        emitter: &crate::eir::build::EirBuilder<'_, C>,
    ) -> Option<EirExpr>
    where
        C: crate::const_eval::ConstValueElaborator + ?Sized,
    {
        let mut expr = expr;
        while let ElabExprNode::Group(inner) = &expr.node {
            expr = inner;
        }
        let ElabExprNode::Ident(name) = &expr.node else {
            return None;
        };
        let reset = self.vars.get(name)?;
        if emitter.static_type_name(&reset.ty) != Some("Reset") {
            return None;
        }
        let reset_domain = emitter.reset_domain(&reset.ty)?;
        self.unique_control("Clock", |var| {
            emitter.first_type_arg(&var.ty).and_then(MirTypeRef::path) == reset_domain.path()
        })
        .ok()
        .flatten()
    }

    pub(crate) fn reset_for_clock<C>(
        &self,
        selected: &EirExpr,
        emitter: &crate::eir::build::EirBuilder<'_, C>,
        span: Span,
    ) -> Result<Option<EirExpr>, crate::CompileError>
    where
        C: crate::const_eval::ConstValueElaborator + ?Sized,
    {
        let Some(clock) = self.vars.values().find(|var| {
            &var.code == selected && emitter.static_type_name(&var.ty) == Some("Clock")
        }) else {
            return Ok(None);
        };
        let clock_domain = emitter.first_type_arg(&clock.ty).and_then(MirTypeRef::path);
        self.unique_control("Reset", |var| {
            emitter.reset_domain(&var.ty).and_then(MirTypeRef::path) == clock_domain
        })
        .map_err(|(first, other)| {
            let clock = selected.fact_key();
            let message = format!("ambiguous reset for clock '{clock}': '{first}' and '{other}' are distinct reset sources; specify rst: explicitly");
            crate::CompileError::lowering_at(crate::EirError::InvalidFormat { message }, span)
        })
    }
}
