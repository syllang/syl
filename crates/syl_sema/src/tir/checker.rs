use super::binding::{BindingKind, BindingRef};
use super::consts::{TirConstEnv, TirConstKind};
use super::design::TirDesign;
use super::phase::Phase;
use super::type_system::{TirType, TirTypeTable, TypeId};
use crate::capability::CapabilityChecker;
use crate::{
    CompileError, HirError, TirError,
    hir::view::HirDesignViewExt,
    hir::{HirBodyExpr, HirDesign, HirExprNode},
    pipeline::StageOutput,
};
use std::{collections::BTreeMap, sync::Arc};
use syl_hir::{DefId, ExprId, HirEnumVariantKey, LocalId};
use syl_span::{Diagnostic, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HardwareBlockMode {
    Normal,
    Control,
}

#[non_exhaustive]
pub struct TypePhaseChecker {
    pub(super) hir: Arc<HirDesign>,
    pub(super) current_owner: Option<DefId>,
    pub(super) current_owner_span: Option<Span>,
    pub(super) type_table: TirTypeTable,
    pub(super) enum_variant_values: BTreeMap<HirEnumVariantKey, u64>,
    pub(super) expr_phases: BTreeMap<ExprId, Phase>,
    pub(super) expr_types: BTreeMap<ExprId, TypeId>,
    pub(super) binding_kinds: BTreeMap<BindingRef, BindingKind>,
    pub(super) binding_types: BTreeMap<BindingRef, TypeId>,
    pub(super) checked_blocks: usize,
    pub(super) checked_exprs: usize,
}

impl TypePhaseChecker {
    pub fn new(hir: Arc<HirDesign>) -> Self {
        Self {
            hir,
            current_owner: None,
            current_owner_span: None,
            type_table: TirTypeTable::new(),
            enum_variant_values: BTreeMap::new(),
            expr_phases: BTreeMap::new(),
            expr_types: BTreeMap::new(),
            binding_kinds: BTreeMap::new(),
            binding_types: BTreeMap::new(),
            checked_blocks: 0,
            checked_exprs: 0,
        }
    }

    pub fn check(self) -> Result<TirDesign, CompileError> {
        self.check_or_collect()
            .map_err(|mut errors| errors.remove(0))
    }

    pub fn check_output(mut self) -> StageOutput<TirDesign> {
        let errors = self.collect_errors();
        let diagnostics = errors.iter().cloned().map(Diagnostic::from).collect();
        StageOutput::new(Some(self.finish()), diagnostics)
    }

    // Result-only adapter for callers that still need immediate error propagation.
    // New code should prefer `check_output` so diagnostics are not forced through Err-only control flow.
    pub fn check_or_collect(mut self) -> Result<TirDesign, Vec<CompileError>> {
        let errors = self.collect_errors();
        if errors.is_empty() {
            return Ok(self.finish());
        }
        Err(errors)
    }

    pub(super) fn hir(&self) -> &HirDesign {
        &self.hir
    }

    pub(super) fn current_owner(&self) -> Result<DefId, CompileError> {
        self.current_owner.ok_or_else(|| {
            CompileError::lowering_at(
                HirError::MissingHirDef {
                    name: "<active owner>".to_string(),
                },
                self.current_owner_span
                    .or_else(|| self.hir.defs.first().map(|def| def.span))
                    .unwrap_or_default(),
            )
        })
    }

    pub(super) fn require_const_bool(
        &self,
        expr: &HirBodyExpr,
        env: &TirConstEnv,
    ) -> Result<(), CompileError> {
        match env.expr_kind(expr, self) {
            Some(TirConstKind::Bool) => Ok(()),
            Some(TirConstKind::Nat) | None => Err(CompileError::lowering_at(
                TirError::ElaborationIfRequiresBool,
                expr.span(),
            )),
        }
    }

    pub(super) fn record_phase(
        &mut self,
        expr: &HirBodyExpr,
        phase: Phase,
    ) -> Result<(), CompileError> {
        let owner = self.current_owner()?;
        let expr_id = self.require_expr_id(owner, expr)?;
        self.expr_phases.insert(expr_id, phase);
        if !self.expr_types.contains_key(&expr_id) {
            let ty = self.infer_expr_type(owner, expr);
            self.record_expr_type_id(expr_id, ty);
        }
        Ok(())
    }

    pub(super) fn record_def_binding(&mut self, id: DefId, kind: BindingKind) {
        self.binding_kinds.insert(BindingRef::Def(id), kind);
    }

    pub(super) fn record_decl_local_binding(
        &mut self,
        name: &str,
        id: Option<LocalId>,
        span: Span,
        kind: BindingKind,
    ) -> Result<LocalId, CompileError> {
        let id = self.require_decl_local_id(name, id, span)?;
        self.binding_kinds.insert(BindingRef::Local(id), kind);
        Ok(id)
    }

    pub(super) fn record_decl_local_type(
        &mut self,
        name: &str,
        id: Option<LocalId>,
        span: Span,
        ty: TirType,
    ) -> Result<(), CompileError> {
        let id = self.require_decl_local_id(name, id, span)?;
        self.record_binding_type(BindingRef::Local(id), ty);
        Ok(())
    }

    fn require_decl_local_id(
        &self,
        name: &str,
        id: Option<LocalId>,
        span: Span,
    ) -> Result<LocalId, CompileError> {
        id.ok_or_else(|| {
            CompileError::lowering_at(
                HirError::MissingHirLocal {
                    name: name.to_string(),
                    start: span.start,
                    end: span.end,
                },
                span,
            )
        })
    }

    pub(super) fn record_expr_type(
        &mut self,
        expr: &HirBodyExpr,
        ty: TirType,
    ) -> Result<(), CompileError> {
        let owner = self.current_owner()?;
        let id = self.require_expr_id(owner, expr)?;
        self.record_expr_type_id(id, ty);
        Ok(())
    }

    fn require_expr_id(&self, owner: DefId, expr: &HirBodyExpr) -> Result<ExprId, CompileError> {
        self.hir.expr_id(owner, expr).ok_or_else(|| {
            CompileError::lowering_at(
                HirError::MissingHirExpr {
                    start: expr.span().start,
                    end: expr.span().end,
                },
                expr.span(),
            )
        })
    }

    fn record_expr_type_id(&mut self, id: ExprId, ty: TirType) {
        let ty = self.type_table.intern(ty);
        self.expr_types.insert(id, ty);
    }

    pub(super) fn record_binding_type(&mut self, binding: BindingRef, ty: TirType) {
        let ty = self.type_table.intern(ty);
        self.binding_types.insert(binding, ty);
    }

    pub(super) fn record_recoverable<T>(
        errors: &mut Vec<CompileError>,
        result: Result<T, CompileError>,
    ) -> Option<T> {
        match result {
            Ok(value) => Some(value),
            Err(error) => {
                errors.push(error);
                None
            }
        }
    }

    pub(super) fn require_const_range(
        &self,
        expr: &HirBodyExpr,
        env: &TirConstEnv,
    ) -> Result<(), CompileError> {
        let HirExprNode::Range { start, end } = &expr.node else {
            return Err(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                expr.span(),
            ));
        };
        self.require_const_nat(start, env, "for range start")?;
        self.require_const_nat(end, env, "for range end")
    }

    pub(super) fn require_const_nat(
        &self,
        expr: &HirBodyExpr,
        env: &TirConstEnv,
        context: &str,
    ) -> Result<(), CompileError> {
        match env.expr_kind(expr, self) {
            Some(TirConstKind::Nat) => Ok(()),
            Some(TirConstKind::Bool) | None => Err(CompileError::lowering_at(
                TirError::RequiresNatExpression {
                    context: context.to_string(),
                },
                expr.span(),
            )),
        }
    }

    pub(super) fn finish(self) -> TirDesign {
        TirDesign {
            hir: self.hir,
            type_table: self.type_table,
            enum_variant_values: self.enum_variant_values,
            expr_phases: self.expr_phases,
            expr_types: self.expr_types,
            binding_kinds: self.binding_kinds,
            binding_types: self.binding_types,
        }
    }

    pub(super) fn collect_errors(&mut self) -> Vec<CompileError> {
        let hir = self.hir.clone();
        let _hir_summary = hir.semantic_summary_count();
        let mut errors = Vec::new();
        for (owner, item) in &hir.consts {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_const(*owner, item, &mut errors) {
                errors.push(error);
            }
        }
        for (owner, item) in &hir.fns {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_fn(*owner, item, &mut errors) {
                errors.push(error);
            }
        }
        for (owner, item) in &hir.enums {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_enum(*owner, item) {
                errors.push(error);
            }
        }
        for (owner, item) in &hir.bundles {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_bundle(*owner, item, &mut errors) {
                errors.push(error);
            }
        }
        for (owner, item) in &hir.interfaces {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_interface(*owner, item, &mut errors) {
                errors.push(error);
            }
        }
        for (owner, callable) in &hir.callables {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_callable(*owner, callable, &mut errors) {
                errors.push(error);
            }
        }
        for (owner, map) in &hir.maps {
            self.current_owner = Some(*owner);
            self.current_owner_span = hir.defs.get(owner.get()).map(|def| def.span);
            if let Err(error) = self.check_map(*owner, map, &mut errors) {
                errors.push(error);
            }
        }
        self.current_owner = None;
        self.current_owner_span = None;
        if errors.is_empty()
            && let Err(error) = CapabilityChecker::new(self.hir()).check()
        {
            errors.push(error);
        }
        errors
    }
}
