use super::super::{BuiltinIntrinsic, BuiltinResolver, TirType, TypePhaseChecker};
use crate::{CompileError, EirError, TirError, hir::view::HirDesignViewExt};
use syl_hir::{HirBodyExpr, HirExprNode, HirLocalKind, HirStmt, resolution::HirResolution};

impl TypePhaseChecker {
    pub(super) fn memory_builtin(&self, expr: &HirBodyExpr) -> Option<BuiltinIntrinsic> {
        let HirExprNode::Call { callee, .. } = &expr.node else {
            return None;
        };
        BuiltinResolver::new(&self.hir, self.current_owner)
            .resolve_call_callee(callee)
            .filter(|b| {
                matches!(
                    b,
                    BuiltinIntrinsic::MemWrite | BuiltinIntrinsic::MemReadSync
                )
            })
    }

    pub(super) fn check_memory_decl(
        &mut self,
        stmt: &HirStmt,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let HirStmt::Mem { id, name, ty, span } = stmt else {
            return Ok(());
        };
        self.record_decl_local_binding(name, *id, *span, super::BindingKind::Local)?;
        let ty = self.type_from_mir_type_ref(self.current_owner()?, ty)?;
        if !matches!(ty, TirType::Array { .. }) {
            errors.push(CompileError::lowering_at(
                EirError::MemoryRequiresArrayType,
                *span,
            ));
        }
        self.record_decl_local_type(name, *id, *span, ty)?;
        Ok(())
    }

    pub(super) fn check_memory_call(
        &mut self,
        expr: &HirBodyExpr,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let HirExprNode::Call { args, .. } = &expr.node else {
            return Ok(());
        };
        if args.len() != 5 || args.iter().any(|a| a.name.is_some()) {
            errors.push(CompileError::lowering_at(
                TirError::TypeMismatch {
                    expected: "five positional memory arguments".into(),
                    actual: format!("{} arguments", args.len()),
                },
                expr.span(),
            ));
            return Ok(());
        }
        let mem = &args[0].value;
        if !self.root_has_kind(mem, |k| matches!(k, HirLocalKind::Mem)) {
            errors.push(CompileError::lowering_at(
                EirError::MemoryTargetMustBeMem {
                    name: self.expr_name(mem).unwrap_or_else(|| "<expression>".into()),
                },
                mem.span(),
            ));
        }
        let owner = self.current_owner()?;
        let read = matches!(
            self.memory_builtin(expr),
            Some(BuiltinIntrinsic::MemReadSync)
        );
        for arg in args.iter().take(if read { 4 } else { 5 }) {
            self.check_hardware_value_expr(&arg.value, errors)?;
        }
        for (i, valid, expected) in [
            (
                1,
                matches!(
                    self.infer_expr_type(owner, &args[1].value),
                    TirType::Clock { .. }
                ),
                "Clock",
            ),
            (
                2,
                matches!(self.infer_expr_type(owner, &args[2].value), TirType::Bit)
                    || matches!(&args[2].value.node, HirExprNode::Int(0 | 1)),
                "Bit",
            ),
            (
                3,
                matches!(
                    self.infer_expr_type(owner, &args[3].value),
                    TirType::UInt { .. } | TirType::Bits { .. } | TirType::Nat
                ),
                "unsigned address",
            ),
        ] {
            if !valid {
                errors.push(CompileError::lowering_at(
                    TirError::TypeMismatch {
                        expected: expected.into(),
                        actual: self.infer_expr_type(owner, &args[i].value).label(),
                    },
                    args[i].span(),
                ));
            }
        }
        if let TirType::Array { elem, .. } = self.infer_expr_type(owner, mem) {
            let target = &args[4].value;
            let actual = self.infer_expr_type(owner, target);
            let valid = if read {
                actual == *elem
            } else {
                self.expr_compatible_for_return(&elem, target)
            };
            if !valid {
                errors.push(CompileError::lowering_at(
                    TirError::TypeMismatch {
                        expected: elem.label(),
                        actual: actual.label(),
                    },
                    target.span(),
                ));
            }
        }
        if read {
            self.check_hardware_drive_target(&args[4].value, errors)?;
        }
        Ok(())
    }

    pub(super) fn root_has_kind(
        &self,
        expr: &HirBodyExpr,
        predicate: impl Fn(HirLocalKind) -> bool,
    ) -> bool {
        let Some(owner) = self.current_owner else {
            return false;
        };
        let mut root = expr;
        while let HirExprNode::Field { base, .. }
        | HirExprNode::Index { base, .. }
        | HirExprNode::Group(base) = &root.node
        {
            root = base;
        }
        let Ok(Some(HirResolution::Local(id))) = self.hir.expr_resolution(owner, root) else {
            return false;
        };
        self.hir.local(id).is_some_and(|l| predicate(l.kind))
    }
}
