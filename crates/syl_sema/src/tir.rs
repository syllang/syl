use crate::TypeId;
#[cfg(test)]
use crate::hir::HirDefKind;
use crate::{
    CompileError, HirError, TirError,
    facts::SemanticFacts,
    hir::{HirBodyExpr, HirDesign, HirExprNode},
    hir_resolve::HirResolution,
    hir_view::HirDesignViewExt,
    tir_const::{TirConstEnv, TirConstKind},
};
use std::{collections::BTreeMap, sync::Arc};
use syl_hir::{DefId, ExprId, LocalId};
use syl_span::Span;

mod body_check;
mod check;
mod return_type;
#[cfg(test)]
mod type_identity_tests;
mod type_support;
pub use type_support::{TirConstTerm, TirType, TirTypeTable};

#[non_exhaustive]
pub struct TirDesign {
    hir: Arc<HirDesign>,
    type_table: TirTypeTable,
    expr_phases: BTreeMap<ExprId, Phase>,
    expr_types: BTreeMap<ExprId, TypeId>,
    binding_kinds: BTreeMap<BindingRef, BindingKind>,
    binding_types: BTreeMap<BindingRef, TypeId>,
    facts: SemanticFacts,
}

impl TirDesign {
    pub fn hir(&self) -> &HirDesign {
        &self.hir
    }

    pub fn debug_dump(&self) -> String {
        format!(
            "tir hir_defs={} hir_locals={} expr_phases={} expr_types={} bindings={} binding_types={}",
            self.hir.defs.len(),
            self.hir.locals.len(),
            self.expr_phases.len(),
            self.expr_types.len(),
            self.binding_kinds.len(),
            self.binding_types.len(),
        )
    }

    pub fn expr_phases(&self) -> &BTreeMap<ExprId, Phase> {
        &self.expr_phases
    }

    pub fn binding_kinds(&self) -> &BTreeMap<BindingRef, BindingKind> {
        &self.binding_kinds
    }

    pub fn type_table(&self) -> &TirTypeTable {
        &self.type_table
    }

    pub fn expr_types(&self) -> &BTreeMap<ExprId, TypeId> {
        &self.expr_types
    }

    pub fn binding_types(&self) -> &BTreeMap<BindingRef, TypeId> {
        &self.binding_types
    }

    pub fn facts(&self) -> &SemanticFacts {
        &self.facts
    }

    pub fn type_count(&self) -> usize {
        self.expr_types.len() + self.binding_types.len()
    }

    #[cfg(test)]
    pub fn binding_type_definition(&self, binding: BindingRef) -> Option<DefId> {
        self.binding_types
            .get(&binding)
            .and_then(|ty| self.type_table.get(*ty))
            .and_then(TirType::definition)
    }

    #[cfg(test)]
    pub fn binding_type_definition_kind(&self, binding: BindingRef) -> Option<HirDefKind> {
        self.binding_types
            .get(&binding)
            .and_then(|ty| self.type_table.get(*ty))
            .and_then(TirType::definition_kind)
    }

    #[cfg(test)]
    pub fn binding_type_label(&self, binding: BindingRef) -> Option<String> {
        self.binding_types
            .get(&binding)
            .and_then(|ty| self.type_table.get(*ty))
            .map(TirType::label)
    }

    pub fn known_type_label(&self, id: ExprId) -> Option<String> {
        self.expr_types
            .get(&id)
            .and_then(|ty| self.type_table.get(*ty))
            .and_then(|ty| (!matches!(ty, TirType::Unknown)).then(|| ty.label()))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BindingRef {
    Def(DefId),
    Local(LocalId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Phase {
    Const,
    Comb,
    Hardware,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BindingKind {
    Const,
    Generic,
    Port,
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuiltinIntrinsic {
    Zero,
}

#[non_exhaustive]
pub struct BuiltinResolver<'a> {
    hir: &'a HirDesign,
    owner: Option<DefId>,
}

impl<'a> BuiltinResolver<'a> {
    pub fn new(hir: &'a HirDesign, owner: Option<DefId>) -> Self {
        Self { hir, owner }
    }

    pub fn resolve_call_callee(&self, callee: &HirBodyExpr) -> Option<BuiltinIntrinsic> {
        let root = self.callee_root(callee)?;
        if self.has_user_resolution(root) {
            return None;
        }
        let HirExprNode::Ident(name) = &root.node else {
            return None;
        };
        self.resolve_name(name)
    }

    fn has_user_resolution(&self, root: &HirBodyExpr) -> bool {
        let Some(owner) = self.owner else {
            return false;
        };
        matches!(
            self.hir.expr_resolution(owner, root),
            Ok(Some(HirResolution::Def(_) | HirResolution::Local(_)))
        )
    }

    fn resolve_name(&self, name: &str) -> Option<BuiltinIntrinsic> {
        match name {
            "zero" => Some(BuiltinIntrinsic::Zero),
            _ => None,
        }
    }

    fn callee_root<'b>(&self, callee: &'b HirBodyExpr) -> Option<&'b HirBodyExpr> {
        let mut current = callee;
        loop {
            match &current.node {
                HirExprNode::Ident(_) => return Some(current),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HardwareBlockMode {
    Normal,
    Control,
}

#[non_exhaustive]
pub struct TypePhaseChecker {
    pub(super) hir: Arc<HirDesign>,
    current_owner: Option<DefId>,
    current_owner_span: Option<Span>,
    type_table: TirTypeTable,
    expr_phases: BTreeMap<ExprId, Phase>,
    expr_types: BTreeMap<ExprId, TypeId>,
    binding_kinds: BTreeMap<BindingRef, BindingKind>,
    binding_types: BTreeMap<BindingRef, TypeId>,
    checked_blocks: usize,
    checked_exprs: usize,
}

impl TypePhaseChecker {
    pub fn new(hir: Arc<HirDesign>) -> Self {
        Self {
            hir,
            current_owner: None,
            current_owner_span: None,
            type_table: TirTypeTable::new(),
            expr_phases: BTreeMap::new(),
            expr_types: BTreeMap::new(),
            binding_kinds: BTreeMap::new(),
            binding_types: BTreeMap::new(),
            checked_blocks: 0,
            checked_exprs: 0,
        }
    }

    fn require_const_bool(
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

    fn record_phase(&mut self, expr: &HirBodyExpr, phase: Phase) -> Result<(), CompileError> {
        let owner = self.current_owner()?;
        let expr_id = self.require_expr_id(owner, expr)?;
        self.expr_phases.insert(expr_id, phase);
        if !self.expr_types.contains_key(&expr_id) {
            let ty = self.infer_expr_type(owner, expr);
            self.record_expr_type_id(expr_id, ty);
        }
        Ok(())
    }

    fn record_def_binding(&mut self, id: DefId, kind: BindingKind) {
        self.binding_kinds.insert(BindingRef::Def(id), kind);
    }

    fn record_decl_local_binding(
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

    fn record_decl_local_type(
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

    fn record_expr_type(&mut self, expr: &HirBodyExpr, ty: TirType) -> Result<(), CompileError> {
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

    fn record_binding_type(&mut self, binding: BindingRef, ty: TirType) {
        let ty = self.type_table.intern(ty);
        self.binding_types.insert(binding, ty);
    }

    fn record_recoverable<T>(
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

    fn current_owner(&self) -> Result<DefId, CompileError> {
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

    fn require_const_range(
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

    fn require_const_nat(
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
}
