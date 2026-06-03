use super::super::{BuiltinIntrinsic, BuiltinResolver, Phase, TypePhaseChecker};
use crate::{
    CompileError, EirError, TirError,
    hir::resolve::HirResolution,
    hir::view::HirDesignViewExt,
    hir::{
        HirBodyExpr, HirCallArg, HirDefKind, HirExprNode, HirMatchArm, HirNamedExpr, HirSelectArm,
    },
    ir::mir::{MirPattern, MirTypeRef},
};
use syl_hir::DefId;
use syl_span::Span;
use syl_syntax::{BinaryOp, UnaryOp};

impl TypePhaseChecker {
    pub(in crate::tir) fn check_map_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.checked_exprs += 1;
        Self::record_recoverable(errors, self.record_phase(expr, Phase::Comb));
        match &expr.node {
            HirExprNode::Place { .. }
            | HirExprNode::Block(_)
            | HirExprNode::CompileError { .. }
            | HirExprNode::Range { .. }
            | HirExprNode::Unsupported => Err(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                expr.span(),
            )),
            HirExprNode::Unary { op, expr } => {
                Self::record_recoverable(errors, self.check_hardware_unary_op(*op, expr.span()));
                self.check_map_expr(expr, errors)
            }
            HirExprNode::Group(expr)
            | HirExprNode::GenericApp { callee: expr, .. }
            | HirExprNode::Field { base: expr, .. }
            | HirExprNode::Index { base: expr, .. } => self.check_map_expr(expr, errors),
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                Self::record_recoverable(errors, self.check_hardware_binary_op(*op, expr.span()));
                self.check_map_expr(left, errors)?;
                self.check_map_expr(right, errors)
            }
            HirExprNode::Call { callee, args } => self.check_map_call(callee, args, errors),
            HirExprNode::Aggregate { ty, fields } => {
                Self::record_recoverable(
                    errors,
                    self.check_aggregate_fields(ty, fields, expr.span()),
                );
                for field in fields {
                    self.check_map_expr(&field.value, errors)?;
                }
                Ok(())
            }
            HirExprNode::Match { expr: target, arms } => {
                Self::record_recoverable(errors, self.check_match_has_arm(arms, expr.span()));
                self.check_map_expr(target, errors)?;
                for arm in arms {
                    Self::record_recoverable(errors, self.check_match_pattern(&arm.pattern));
                    self.check_map_expr(&arm.value, errors)?;
                }
                Ok(())
            }
            HirExprNode::Select { arms, .. } => {
                Self::record_recoverable(errors, self.check_select_has_default(arms, expr.span()));
                for arm in arms {
                    self.check_map_expr(&arm.pattern, errors)?;
                    self.check_map_expr(&arm.value, errors)?;
                }
                Ok(())
            }
            HirExprNode::Ident(_) | HirExprNode::Int(_) | HirExprNode::Str(_) => Ok(()),
            HirExprNode::Bool(_) => Err(CompileError::lowering_at(
                TirError::BoolInHardwareValue,
                expr.span(),
            )),
            _ => Err(CompileError::lowering_at(
                TirError::InvalidElaborationExpression,
                expr.span(),
            )),
        }
    }

    pub(in crate::tir) fn check_hardware_value_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        Self::record_recoverable(errors, self.record_phase(expr, Phase::Hardware));
        match &expr.node {
            HirExprNode::Ident(_) | HirExprNode::Int(_) | HirExprNode::Str(_) => Ok(()),
            HirExprNode::Bool(_) => {
                errors.push(CompileError::lowering_at(
                    TirError::BoolInHardwareValue,
                    expr.span(),
                ));
                Ok(())
            }
            HirExprNode::Unary { op, expr } => {
                Self::record_recoverable(errors, self.check_hardware_unary_op(*op, expr.span()));
                self.check_hardware_value_expr(expr, errors)
            }
            HirExprNode::Group(expr) => self.check_hardware_value_expr(expr, errors),
            HirExprNode::GenericApp { callee, .. } => {
                self.check_hardware_value_expr(callee, errors)
            }
            HirExprNode::Field { base, .. } => self.check_hardware_value_expr(base, errors),
            HirExprNode::Index { base, index } => {
                self.check_hardware_value_expr(base, errors)?;
                self.check_hardware_value_expr(index, errors)
            }
            HirExprNode::Binary {
                op, left, right, ..
            } => {
                Self::record_recoverable(errors, self.check_hardware_binary_op(*op, expr.span()));
                self.check_hardware_value_expr(left, errors)?;
                self.check_hardware_value_expr(right, errors)
            }
            HirExprNode::Call { callee, args } => {
                self.check_hardware_value_call(callee, args, errors)
            }
            HirExprNode::Place { .. }
            | HirExprNode::Block(_)
            | HirExprNode::CompileError { .. }
            | HirExprNode::Range { .. }
            | HirExprNode::Unsupported => {
                errors.push(CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    expr.span(),
                ));
                Ok(())
            }
            HirExprNode::Aggregate { ty, fields } => {
                Self::record_recoverable(
                    errors,
                    self.check_aggregate_fields(ty, fields, expr.span()),
                );
                for field in fields {
                    self.check_hardware_value_expr(&field.value, errors)?;
                }
                Ok(())
            }
            HirExprNode::Match { expr: target, arms } => {
                Self::record_recoverable(errors, self.check_match_has_arm(arms, expr.span()));
                self.check_hardware_value_expr(target, errors)?;
                for arm in arms {
                    Self::record_recoverable(errors, self.check_match_pattern(&arm.pattern));
                    self.check_hardware_value_expr(&arm.value, errors)?;
                }
                Ok(())
            }
            HirExprNode::Select { arms, .. } => {
                Self::record_recoverable(errors, self.check_select_has_default(arms, expr.span()));
                for arm in arms {
                    if !self.is_default_select_pattern(&arm.pattern) {
                        self.check_hardware_value_expr(&arm.pattern, errors)?;
                    }
                    self.check_hardware_value_expr(&arm.value, errors)?;
                }
                Ok(())
            }
            _ => {
                errors.push(CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    expr.span(),
                ));
                Ok(())
            }
        }
    }

    pub(in crate::tir) fn check_hardware_value_call(
        &mut self,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        if let Some(name) = self.hardware_generator_name(callee) {
            errors.push(CompileError::lowering_at(
                EirError::HardwareGeneratorCallInExpression { name },
                callee.span(),
            ));
        } else {
            self.check_generator_args(args, errors)?;
            if let Some(call) = Self::record_recoverable(
                errors,
                self.checked_extension_method_call(self.current_owner()?, callee),
            )
            .flatten()
            {
                if self.hir.def_kind(call.method) == Some(HirDefKind::Map) {
                    return Ok(());
                }
            }
            let Some(name) = self.expr_name(callee) else {
                errors.push(CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    callee.span(),
                ));
                return Ok(());
            };
            let builtin =
                BuiltinResolver::new(&self.hir, self.current_owner).resolve_call_callee(callee);
            if matches!(
                builtin,
                Some(BuiltinIntrinsic::HighZ | BuiltinIntrinsic::Zero)
            ) || self.is_map_callee(callee)
            {
                return Ok(());
            }
            if matches!(builtin, Some(BuiltinIntrinsic::Assert)) {
                errors.push(CompileError::lowering_at(
                    EirError::AssertionStatementOnly,
                    callee.span(),
                ));
                for arg in args {
                    self.check_hardware_value_expr(&arg.value, errors)?;
                }
                return Ok(());
            }
            errors.push(CompileError::lowering_at(
                EirError::UnknownHardwareValueCall { name },
                callee.span(),
            ));
        }
        for arg in args {
            self.check_hardware_value_expr(&arg.value, errors)?;
        }
        Ok(())
    }

    fn check_map_call(
        &mut self,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        if let Some(name) = self.hardware_generator_name(callee) {
            errors.push(CompileError::lowering_at(
                EirError::HardwareGeneratorCallInMap { name },
                callee.span(),
            ));
        } else {
            let Some(name) = self.expr_name(callee) else {
                errors.push(CompileError::lowering_at(
                    TirError::InvalidElaborationExpression,
                    callee.span(),
                ));
                for arg in args {
                    self.check_map_expr(&arg.value, errors)?;
                }
                return Ok(());
            };
            let builtin =
                BuiltinResolver::new(&self.hir, self.current_owner).resolve_call_callee(callee);
            if let Some(call) = Self::record_recoverable(
                errors,
                self.checked_extension_method_call(self.current_owner()?, callee),
            )
            .flatten()
            {
                if self.hir.def_kind(call.method) == Some(HirDefKind::Map) {
                    for arg in args {
                        self.check_map_expr(&arg.value, errors)?;
                    }
                    return Ok(());
                }
            }
            if !matches!(
                builtin,
                Some(BuiltinIntrinsic::HighZ | BuiltinIntrinsic::Zero)
            ) && !self.is_map_callee(callee)
            {
                if matches!(builtin, Some(BuiltinIntrinsic::Assert)) {
                    errors.push(CompileError::lowering_at(
                        EirError::AssertionStatementOnly,
                        callee.span(),
                    ));
                } else {
                    errors.push(CompileError::lowering_at(
                        EirError::UnknownHardwareValueCall { name },
                        callee.span(),
                    ));
                }
            }
        }
        for arg in args {
            self.check_map_expr(&arg.value, errors)?;
        }
        Ok(())
    }

    pub(in crate::tir) fn check_generator_args(
        &mut self,
        args: &[HirCallArg],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        for arg in args {
            self.check_hardware_value_expr(&arg.value, errors)?;
        }
        Ok(())
    }

    pub(in crate::tir) fn check_place_expr(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        Self::record_recoverable(errors, self.record_phase(expr, Phase::Hardware));
        match &expr.node {
            HirExprNode::Ident(_) => Ok(()),
            HirExprNode::Field { base, .. } => self.check_place_expr(base, errors),
            HirExprNode::Index { base, index } => {
                self.check_place_expr(base, errors)?;
                self.check_hardware_value_expr(index, errors)
            }
            HirExprNode::Group(expr) => self.check_place_expr(expr, errors),
            _ => {
                errors.push(CompileError::lowering_at(
                    EirError::UnsupportedHardwareValueExpression,
                    expr.span(),
                ));
                Ok(())
            }
        }
    }

    fn check_hardware_unary_op(&self, op: UnaryOp, span: Span) -> Result<(), CompileError> {
        if matches!(op, UnaryOp::Not) {
            return Err(CompileError::lowering_at(
                TirError::SoftwareOperatorInHardware {
                    op: <&'static str>::from(op).to_string(),
                },
                span,
            ));
        }
        Ok(())
    }

    fn check_hardware_binary_op(&self, op: BinaryOp, span: Span) -> Result<(), CompileError> {
        if matches!(
            op,
            BinaryOp::OrOr
                | BinaryOp::AndAnd
                | BinaryOp::EqEq
                | BinaryOp::NotEq
                | BinaryOp::Lt
                | BinaryOp::LtEq
                | BinaryOp::Gt
                | BinaryOp::GtEq
        ) {
            return Err(CompileError::lowering_at(
                TirError::SoftwareOperatorInHardware {
                    op: <&'static str>::from(op).to_string(),
                },
                span,
            ));
        }
        Ok(())
    }

    fn check_select_has_default(
        &self,
        arms: &[HirSelectArm],
        span: Span,
    ) -> Result<(), CompileError> {
        if arms
            .iter()
            .any(|arm| self.is_default_select_pattern(&arm.pattern))
        {
            return Ok(());
        }
        Err(CompileError::lowering_at(
            TirError::SelectRequiresDefault,
            span,
        ))
    }

    fn check_match_has_arm(&self, arms: &[HirMatchArm], span: Span) -> Result<(), CompileError> {
        if !arms.is_empty() {
            return Ok(());
        }
        Err(CompileError::lowering_at(TirError::MatchRequiresArm, span))
    }

    fn check_match_pattern(&self, pattern: &MirPattern) -> Result<(), CompileError> {
        match pattern {
            MirPattern::Bool(_, span) => Err(CompileError::lowering_at(
                TirError::BoolInHardwareValue,
                *span,
            )),
            MirPattern::Wildcard(_)
            | MirPattern::Ident(_, _)
            | MirPattern::Int(_, _)
            | MirPattern::Path(_, _) => Ok(()),
            MirPattern::Unsupported(span) => Err(CompileError::lowering_at(
                EirError::UnsupportedHardwareValueExpression,
                *span,
            )),
            _ => Ok(()),
        }
    }

    fn check_aggregate_fields(
        &self,
        ty: &MirTypeRef,
        fields: &[HirNamedExpr],
        span: Span,
    ) -> Result<(), CompileError> {
        let owner = self.current_owner()?;
        let aggregate_ty = self.type_from_mir_type_ref(owner, ty)?;
        let Some(bundle_def) = aggregate_ty.definition() else {
            return Err(CompileError::lowering_at(
                EirError::UnsupportedHardwareValueExpression,
                span,
            ));
        };
        let Some(bundle) = self.hir.bundles.get(&bundle_def) else {
            return Err(CompileError::lowering_at(
                EirError::UnsupportedHardwareValueExpression,
                span,
            ));
        };
        for field in fields {
            if !bundle.fields.iter().any(|decl| decl.name == field.name) {
                return Err(CompileError::lowering_at(
                    TirError::UnknownAggregateField {
                        ty: aggregate_ty.label(),
                        field: field.name.clone(),
                    },
                    field.value.span(),
                ));
            }
        }
        for field in &bundle.fields {
            if !fields.iter().any(|provided| provided.name == field.name) {
                return Err(CompileError::lowering_at(
                    TirError::MissingAggregateField {
                        ty: aggregate_ty.label(),
                        field: field.name.clone(),
                    },
                    span,
                ));
            }
        }
        Ok(())
    }

    fn is_default_select_pattern(&self, expr: &HirBodyExpr) -> bool {
        matches!(&expr.node, HirExprNode::Ident(name) if name == "default")
    }

    pub(in crate::tir) fn hardware_generator_name(&self, callee: &HirBodyExpr) -> Option<String> {
        let root = self.callee_root(callee)?;
        let owner = self.current_owner().ok()?;
        match self.hir.expr_resolution(owner, root).ok()? {
            Some(HirResolution::Def(def)) if self.is_generator_def(def) => {
                self.hir.def_name(def).map(str::to_string)
            }
            _ => None,
        }
    }

    fn is_map_callee(&self, callee: &HirBodyExpr) -> bool {
        let Ok(owner) = self.current_owner() else {
            return false;
        };
        self.map_callee_def(owner, callee).is_some()
    }

    pub(in crate::tir) fn map_callee_def(
        &self,
        owner: DefId,
        callee: &HirBodyExpr,
    ) -> Option<DefId> {
        let root = self.callee_root(callee)?;
        match self.hir.expr_resolution(owner, root).ok()? {
            Some(HirResolution::Def(def)) if self.hir.def_kind(def) == Some(HirDefKind::Map) => {
                Some(def)
            }
            _ => None,
        }
    }

    fn is_generator_def(&self, def: DefId) -> bool {
        matches!(
            self.hir.def_kind(def),
            Some(HirDefKind::Cell | HirDefKind::ExternCell)
        )
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

    fn expr_name(&self, expr: &HirBodyExpr) -> Option<String> {
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::Ident(name) => return Some(name.clone()),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }
}
