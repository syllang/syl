use super::{EirItem, EirModule, EirReset};
use crate::{
    CellBoundarySummary, CompileError, EirError,
    eir_expr::{EirExpr, EirSelectArm},
    eir_origin::EirOrigin,
    eir_place::EirPlace,
};

#[non_exhaustive]
pub(crate) struct EirValidator<'a> {
    modules: &'a [EirModule],
}

impl<'a> EirValidator<'a> {
    pub(crate) fn new(modules: &'a [EirModule]) -> Self {
        Self { modules }
    }

    pub(crate) fn validate(&self) -> Result<(), CompileError> {
        for module in self.modules {
            self.check_items(module.items())?;
        }
        Ok(())
    }

    fn check_items(&self, items: &[EirItem]) -> Result<(), CompileError> {
        for item in items {
            self.check_item(item)?;
        }
        Ok(())
    }

    fn check_item(&self, item: &EirItem) -> Result<(), CompileError> {
        match item {
            EirItem::StaticParam { value, origin, .. } => self.check_expr(value, origin),
            EirItem::Signal { .. } | EirItem::Storage { .. } => Ok(()),
            EirItem::Drive {
                lhs,
                rhs,
                reads,
                origin,
            } => {
                self.check_place(lhs, origin)?;
                self.check_expr(rhs, origin)?;
                self.check_exprs(reads, origin)
            }
            EirItem::ClockedStorage {
                clock,
                target,
                reset,
                next,
                reads,
                origin,
            } => {
                self.check_expr(clock, origin)?;
                self.check_place(target, origin)?;
                self.check_reset(reset.as_deref(), origin)?;
                self.check_expr(next, origin)?;
                self.check_exprs(reads, origin)
            }
            EirItem::CellExpansion(expansion) => self.check_items(expansion.items()),
            EirItem::CellBoundary(boundary) => self.check_cell_boundary(boundary),
            EirItem::Instance(instance) => {
                for connection in instance.connections() {
                    self.check_expr(connection.actual(), instance.origin())?;
                }
                Ok(())
            }
            EirItem::SymbolicStaticIf {
                cond,
                then_items,
                else_items,
                origin,
                ..
            } => {
                self.check_expr(cond, origin)?;
                self.check_items(then_items)?;
                self.check_items(else_items)
            }
            EirItem::SymbolicStaticFor {
                start,
                end,
                items,
                origin,
                ..
            } => {
                self.check_expr(start, origin)?;
                self.check_expr(end, origin)?;
                self.check_items(items)
            }
            EirItem::InitialError { message, origin } => self.check_expr(message, origin),
        }
    }

    fn check_cell_boundary(&self, boundary: &CellBoundarySummary) -> Result<(), CompileError> {
        boundary.require_available().map(|_| ())
    }

    fn check_reset(
        &self,
        reset: Option<&EirReset>,
        origin: &EirOrigin,
    ) -> Result<(), CompileError> {
        if let Some(reset) = reset {
            self.check_expr(reset.condition(), origin)?;
            self.check_expr(reset.value(), origin)?;
        }
        Ok(())
    }

    fn check_place(&self, place: &EirPlace, origin: &EirOrigin) -> Result<(), CompileError> {
        match place {
            EirPlace::Ident(_) => Ok(()),
            EirPlace::Slice { base, high, low } => {
                self.check_place(base, origin)?;
                self.check_expr(high.expr(), origin)?;
                self.check_expr(low.expr(), origin)
            }
            EirPlace::IndexedPartSelect { base, index, width } => {
                self.check_place(base, origin)?;
                self.check_expr(index, origin)?;
                self.check_expr(width.expr(), origin)
            }
            EirPlace::Index { base, index } => {
                self.check_place(base, origin)?;
                self.check_expr(index, origin)
            }
        }
    }

    fn check_exprs(&self, exprs: &[EirExpr], origin: &EirOrigin) -> Result<(), CompileError> {
        for expr in exprs {
            self.check_expr(expr, origin)?;
        }
        Ok(())
    }

    fn check_expr(&self, expr: &EirExpr, origin: &EirOrigin) -> Result<(), CompileError> {
        match expr {
            EirExpr::Unsupported { .. } => Err(CompileError::lowering_at(
                EirError::UnsupportedHardwareValueExpression,
                origin.span(),
            )),
            EirExpr::Unary { expr, .. } => self.check_expr(expr, origin),
            EirExpr::Binary { left, right, .. } => {
                self.check_expr(left, origin)?;
                self.check_expr(right, origin)
            }
            EirExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                self.check_expr(cond, origin)?;
                self.check_expr(then_value, origin)?;
                self.check_expr(else_value, origin)
            }
            EirExpr::Select { arms, default, .. } => {
                for arm in arms {
                    self.check_select_arm(arm, origin)?;
                }
                self.check_expr(default, origin)
            }
            EirExpr::Concat(parts) => self.check_exprs(parts, origin),
            EirExpr::Slice { value, high, low } => {
                self.check_expr(value, origin)?;
                self.check_expr(high.expr(), origin)?;
                self.check_expr(low.expr(), origin)
            }
            EirExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => {
                self.check_expr(value, origin)?;
                self.check_expr(index, origin)?;
                self.check_expr(width.expr(), origin)
            }
            EirExpr::Index { value, index } => {
                self.check_expr(value, origin)?;
                self.check_expr(index, origin)
            }
            EirExpr::Call { args, .. } => self.check_exprs(args, origin),
            EirExpr::Ident(_)
            | EirExpr::Int(_)
            | EirExpr::Bool(_)
            | EirExpr::Str(_)
            | EirExpr::HighZ
            | EirExpr::Zero => Ok(()),
        }
    }

    fn check_select_arm(&self, arm: &EirSelectArm, origin: &EirOrigin) -> Result<(), CompileError> {
        self.check_expr(arm.guard(), origin)?;
        self.check_expr(arm.value(), origin)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        CellBoundarySummary, DriverError, LoweringError,
        eir::{
            EirDesign, EirDesignComposer, EirDriveKind, EirFactCollector, EirItem, EirModule,
            EirRawDesign,
        },
        eir_guard::EirGuard,
        eir_origin::EirOrigin,
        eir_place::EirPlace,
    };
    use std::sync::Arc;
    use syl_sema::{
        OpaqueSummaryTable,
        cell_summary::{CellSummaryDeclaration, CellSummaryRegistry, HwOrigin, HwPlace},
    };
    use syl_span::{SourceId, Span};

    fn validated_design(modules: Vec<EirModule>) -> Result<EirDesign, CompileError> {
        let raw = Arc::new(EirRawDesign::new(modules));
        EirValidator::new(raw.modules()).validate()?;
        let facts = Arc::new(EirFactCollector::collect(
            raw.modules(),
            &OpaqueSummaryTable::new(),
        )?);
        Ok(EirDesignComposer::compose(raw, facts))
    }

    #[test]
    fn rejects_unsupported_expr_before_collecting_facts() {
        let span = Span::new_in(SourceId::new(0), 10, 20);
        let origin = EirOrigin::new(span, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::Drive {
                lhs: EirPlace::Ident("y".to_string()),
                rhs: EirExpr::unsupported("test unsupported"),
                reads: Vec::new(),
                origin,
            }],
        );

        let validation = {
            let raw = EirRawDesign::new(vec![module]);
            EirValidator::new(raw.modules()).validate()
        };
        let error = match validation {
            Ok(_) => panic!("unsupported EIR expression must be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.diagnostic().span, span);
    }

    #[test]
    fn rejects_unsupported_expr_inside_place_projection() {
        let span = Span::new_in(SourceId::new(1), 30, 40);
        let origin = EirOrigin::new(span, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::ClockedStorage {
                clock: EirExpr::ident("clk"),
                target: EirPlace::Index {
                    base: Box::new(EirPlace::Ident("r".to_string())),
                    index: EirExpr::unsupported("bad index"),
                },
                reset: None,
                next: EirExpr::ident("r"),
                reads: Vec::new(),
                origin,
            }],
        );

        let validation = {
            let raw = EirRawDesign::new(vec![module]);
            EirValidator::new(raw.modules()).validate()
        };
        let error = match validation {
            Ok(_) => panic!("unsupported EIR place index must be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.diagnostic().span, span);
    }

    #[test]
    fn accepts_supported_exprs_and_collects_facts() {
        let span = Span::new_in(SourceId::new(2), 50, 60);
        let origin = EirOrigin::new(span, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::Drive {
                lhs: EirPlace::Ident("y".to_string()),
                rhs: EirExpr::ident("x"),
                reads: vec![EirExpr::ident("x")],
                origin,
            }],
        );

        let design = match validated_design(vec![module]) {
            Ok(design) => design,
            Err(error) => panic!("supported EIR must validate: {error}"),
        };

        assert_eq!(design.drives().len(), 1);
        assert!(matches!(
            design.drives()[0].kind(),
            EirDriveKind::Continuous
        ));
        assert_eq!(design.drives()[0].guard(), &EirGuard::root());
        assert_eq!(design.reads().len(), 1);
    }

    #[test]
    fn rejects_missing_opaque_cell_boundary_summary() {
        let span = Span::new_in(SourceId::new(3), 70, 80);
        let origin = HwOrigin::new(span.source, span.start, span.end, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::CellBoundary(CellBoundarySummary::missing(
                "VendorCell",
                "u_vendor",
                origin,
            ))],
        );

        let validation = {
            let raw = EirRawDesign::new(vec![module]);
            EirValidator::new(raw.modules()).validate()
        };
        let error = match validation {
            Ok(_) => panic!("missing opaque cell summary must be rejected"),
            Err(error) => error,
        };

        assert!(matches!(
            error,
            CompileError::Lowering { ref kind, .. }
                if matches!(
                    kind.as_ref(),
                    LoweringError::Driver(DriverError::MissingCellSummary {
                        callable,
                        instance,
                        status,
                    })
                        if callable == "VendorCell"
                            && instance == "u_vendor"
                            && status == "missing"
                )
        ));
        assert_eq!(error.diagnostic().span, span);
    }

    #[test]
    fn accepts_cell_boundary_summary_loaded_from_registry() {
        let summary_origin = HwOrigin::new(SourceId::new(4), 90, 100, Vec::new());
        let boundary_span = Span::new_in(SourceId::new(5), 110, 120);
        let boundary_origin = HwOrigin::new(
            boundary_span.source,
            boundary_span.start,
            boundary_span.end,
            Vec::new(),
        );

        let mut declaration =
            CellSummaryDeclaration::exact("VendorCell", "u_vendor", summary_origin.clone());
        declaration.add_drive(HwPlace::Ident("u_vendor.out".to_string()));
        let registry = CellSummaryRegistry::from_iter([declaration]);
        let resolved = CellBoundarySummary::missing("VendorCell", "u_vendor", boundary_origin)
            .resolve_with(&registry);

        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::CellBoundary(resolved)],
        );

        let design = match validated_design(vec![module]) {
            Ok(design) => design,
            Err(error) => panic!("resolved cell boundary summary must validate: {error}"),
        };

        assert_eq!(design.modules().len(), 1);
    }
}
