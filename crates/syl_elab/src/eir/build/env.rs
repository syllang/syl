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
    ) {
        self.expansion_stack
            .push(EirExpansion::new(callable, instance, span));
    }

    pub(crate) fn single_by_type<C>(
        &self,
        type_name: &str,
        _emitter: &crate::eir::build::EirBuilder<'_, C>,
    ) -> Option<EirExpr>
    where
        C: crate::const_eval::ConstValueElaborator + ?Sized,
    {
        let names = self.vars_by_static_type.get(type_name)?;
        if names.len() != 1 {
            return None;
        }
        self.vars.get(&names[0]).map(|var| var.code.clone())
    }

    pub(crate) fn clock_for_elab_reset_expr<C>(
        &self,
        expr: &ElabExpr,
        emitter: &crate::eir::build::EirBuilder<'_, C>,
    ) -> Option<EirExpr>
    where
        C: crate::const_eval::ConstValueElaborator + ?Sized,
    {
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
