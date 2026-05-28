use super::{
    activity::DriverSignalActivityChecker,
    bounds::DriverBoundsChecker,
    coverage::{DriverCompletenessChecker, DriverReadCompletenessChecker},
    guard::DriverGuardSet,
    tristate::DriveConflict,
};
use crate::{
    CompileError, DriverError,
    driver::place::DriverPlace,
    driver::{CreateKind, DriveEffect, DriveFact, DriverFacts, ReadFact},
    eir::{EirDesign, EirOrigin},
};
use syl_span::Span;

#[non_exhaustive]
pub(crate) struct DriverDrcReport {
    module_count: usize,
    drive_count: usize,
    read_count: usize,
    create_count: usize,
}

impl DriverDrcReport {
    fn new(eir: &EirDesign, facts: &DriverFacts) -> Self {
        Self {
            module_count: eir.modules().len(),
            drive_count: facts.drives().len(),
            read_count: facts.reads().len(),
            create_count: facts.creates().len(),
        }
    }

    pub(crate) fn module_count(&self) -> usize {
        self.module_count
    }

    pub(crate) fn drive_count(&self) -> usize {
        self.drive_count
    }

    pub(crate) fn read_count(&self) -> usize {
        self.read_count
    }

    pub(crate) fn create_count(&self) -> usize {
        self.create_count
    }
}

#[non_exhaustive]
pub(crate) struct DriverDrcChecker<'a> {
    eir: &'a EirDesign,
    facts: &'a DriverFacts,
    errors: Vec<CompileError>,
}

impl<'a> DriverDrcChecker<'a> {
    pub(crate) fn new(eir: &'a EirDesign, facts: &'a DriverFacts) -> Self {
        Self {
            eir,
            facts,
            errors: Vec::new(),
        }
    }

    pub(crate) fn check_collect(mut self) -> Result<DriverDrcReport, Vec<CompileError>> {
        self.check_drive_facts();
        self.check_read_facts();
        self.collect_out_completeness();
        self.collect_read_completeness();
        self.collect_signal_activity();
        if !self.errors.is_empty() {
            return Err(self.errors);
        }
        Ok(DriverDrcReport::new(self.eir, self.facts))
    }

    fn check_drive_facts(&mut self) {
        let mut accepted_nexts: Vec<&DriveFact> = Vec::new();
        for (index, drive) in self.facts.drives().iter().enumerate() {
            if let Some(error) = self.check_next_target(drive, &accepted_nexts) {
                self.errors.push(error);
                continue;
            }
            if let Some(error) = self.check_continuous_target(drive) {
                self.errors.push(error);
                continue;
            }
            if let Some(error) = self.check_drive_bounds(drive) {
                self.errors.push(error);
                continue;
            }
            if let Some(error) = self.check_drive_conflict(index, drive) {
                self.errors.push(error);
                continue;
            }
            if matches!(drive.effect(), DriveEffect::Next { .. }) {
                accepted_nexts.push(drive);
            }
        }
    }

    fn check_read_facts(&mut self) {
        for read in self.facts.reads() {
            if let Some(error) = self.check_read_bounds(read) {
                self.errors.push(error);
            }
        }
    }

    fn check_next_target(
        &self,
        drive: &DriveFact,
        accepted_nexts: &[&DriveFact],
    ) -> Option<CompileError> {
        let DriveEffect::Next { storage_target } = drive.effect() else {
            return None;
        };
        if !self.is_storage_target(drive.module(), storage_target) {
            return Some(CompileError::driver_error(
                DriverError::NextTargetIsNotReg {
                    name: storage_target.display(),
                },
                drive.origin().span(),
            ));
        }
        let previous = accepted_nexts.iter().copied().find(|claim| {
            claim.module() == drive.module()
                && matches!(
                    claim.effect(),
                    DriveEffect::Next {
                        storage_target: previous_target,
                    } if previous_target == storage_target
                )
                && claim.guard() == drive.guard()
        })?;
        Some(CompileError::driver_error_with_related(
            DriverError::DuplicateNextDriver {
                name: storage_target.display(),
            },
            drive.origin().span(),
            conflict_related(
                previous.origin(),
                drive.origin(),
                "previous next driver claim",
                "conflicting next driver claim",
            ),
        ))
    }

    fn check_continuous_target(&self, drive: &DriveFact) -> Option<CompileError> {
        if !matches!(drive.effect(), DriveEffect::Continuous) {
            return None;
        }
        let name = self.storage_root_name(drive.module(), drive.target_place())?;
        Some(CompileError::driver_error(
            DriverError::ContinuousDriveTargetIsReg { name },
            drive.origin().span(),
        ))
    }

    fn check_drive_bounds(&self, drive: &DriveFact) -> Option<CompileError> {
        let target = match drive.effect() {
            DriveEffect::Continuous => drive.target_place(),
            DriveEffect::Next { storage_target } => storage_target,
        };
        DriverBoundsChecker::new(self.facts.objects())
            .check_place(target, drive.guard(), drive.origin())
            .err()
    }

    fn check_drive_conflict(&self, index: usize, drive: &DriveFact) -> Option<CompileError> {
        let previous = self.facts.drives()[..index].iter().find(|claim| {
            claim.module() == drive.module()
                && claim.target_place().overlaps(drive.target_place())
                && !DriverGuardSet::new(claim.guard(), drive.guard()).is_mutually_exclusive()
                && DriveConflict::new(claim, drive).can_conflict()
        })?;
        Some(CompileError::driver_error_with_related(
            DriverError::DuplicateHardwareDriver {
                name: drive.target_place().display(),
            },
            drive.origin().span(),
            conflict_related(
                previous.origin(),
                drive.origin(),
                "previous driver claim",
                "conflicting driver claim",
            ),
        ))
    }

    fn check_read_bounds(&self, read: &ReadFact) -> Option<CompileError> {
        DriverBoundsChecker::new(self.facts.objects())
            .check_place(read.source_place(), read.guard(), read.origin())
            .err()
    }

    fn collect_out_completeness(&mut self) {
        let checker =
            DriverCompletenessChecker::new(self.eir, self.facts.objects(), self.facts.drives());
        self.errors.extend(checker.collect_errors());
    }

    fn collect_read_completeness(&mut self) {
        let checker = DriverReadCompletenessChecker::new(
            self.facts.objects(),
            self.facts.drives(),
            self.facts.reads(),
            self.facts.creates(),
        );
        self.errors.extend(checker.collect_errors());
    }

    fn collect_signal_activity(&mut self) {
        self.errors.extend(
            DriverSignalActivityChecker::new(
                self.facts.creates(),
                self.facts.drives(),
                self.facts.objects(),
            )
            .collect_errors(),
        );
    }

    fn is_storage_target(&self, module: &str, target: &DriverPlace) -> bool {
        let DriverPlace::Object(object) = target else {
            return false;
        };
        self.facts.creates().iter().any(|create| {
            create.module() == module
                && create.object_id() == object.id()
                && matches!(create.kind(), CreateKind::Storage)
        })
    }

    fn storage_root_name(&self, module: &str, target: &DriverPlace) -> Option<String> {
        match target {
            DriverPlace::Object(object) => self
                .is_storage_target(module, target)
                .then(|| object.name().to_string()),
            DriverPlace::Slice { base, .. }
            | DriverPlace::IndexedPartSelect { base, .. }
            | DriverPlace::Index { base, .. } => self.storage_root_name(module, base),
            DriverPlace::Expr(_) => None,
        }
    }
}

fn conflict_related(
    previous: &EirOrigin,
    conflicting: &EirOrigin,
    previous_label: &str,
    conflicting_label: &str,
) -> Vec<(Span, String)> {
    let mut related = Vec::new();
    push_related(&mut related, previous.span(), previous_label.to_string());
    push_related(
        &mut related,
        conflicting.span(),
        conflicting_label.to_string(),
    );
    push_expansion_stack(&mut related, previous, "previous");
    push_expansion_stack(&mut related, conflicting, "conflicting");
    related
}

fn push_expansion_stack(related: &mut Vec<(Span, String)>, origin: &EirOrigin, side: &str) {
    for expansion in origin.expansion_stack() {
        push_related(
            related,
            expansion.span(),
            format!(
                "{side} expansion {} via {}",
                expansion.callable(),
                expansion.instance()
            ),
        );
    }
}

fn push_related(related: &mut Vec<(Span, String)>, span: Span, message: String) {
    if related.iter().any(|(existing_span, existing_message)| {
        *existing_span == span && *existing_message == message
    }) {
        return;
    }
    related.push((span, message));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LoweringError,
        eir::{
            EirBinaryOp, EirExpansion, EirExpr, EirGuard, EirOrigin, EirPlace, EirSelectArm,
            EirSelectMode, EirUnaryOp,
        },
        eir::{
            EirDesign, EirDesignComposer, EirFactCollector, EirItem, EirModule, EirRawDesign,
            EirSignalActivity, EirValidator,
        },
    };
    use std::sync::Arc;
    use syl_sema::OpaqueSummaryTable;
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

    fn origin() -> EirOrigin {
        EirOrigin::new(Span::new_in(SourceId::new(0), 0, 1), Vec::new())
    }

    fn tri_drive(target: &str, enable: EirExpr, value: u64) -> EirItem {
        EirItem::Drive {
            lhs: EirPlace::Ident(target.to_string()),
            rhs: EirExpr::select(
                EirSelectMode::Priority,
                vec![EirSelectArm::new(enable, EirExpr::Int(value))],
                EirExpr::HighZ,
            ),
            reads: Vec::new(),
            origin: origin(),
        }
    }

    fn optional_signal(name: &str) -> EirItem {
        EirItem::Signal {
            width: "1".into(),
            name: name.to_string(),
            activity: EirSignalActivity::Optional,
            origin: origin(),
        }
    }

    #[test]
    fn tristate_drivers_with_opposite_enables_do_not_conflict() {
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![
                optional_signal("bus"),
                tri_drive("bus", EirExpr::ident("en"), 1),
                tri_drive(
                    "bus",
                    EirExpr::unary(EirUnaryOp::Not, EirExpr::ident("en")),
                    0,
                ),
            ],
        );
        let design = validated_design(vec![module]).expect("test EIR should assemble");
        let facts = crate::driver::DriverFactsCollector::new(&design)
            .collect()
            .expect("facts pass should succeed");

        DriverDrcChecker::new(&design, &facts)
            .check_collect()
            .expect("opposite tri-state enables must not conflict");
    }

    #[test]
    fn tristate_drivers_with_overlapping_enables_conflict() {
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![
                optional_signal("bus"),
                tri_drive("bus", EirExpr::ident("en"), 1),
                tri_drive(
                    "bus",
                    EirExpr::binary(
                        EirBinaryOp::AndAnd,
                        EirExpr::ident("en"),
                        EirExpr::ident("grant"),
                    ),
                    0,
                ),
            ],
        );
        let design = validated_design(vec![module]).expect("test EIR should assemble");
        let facts = crate::driver::DriverFactsCollector::new(&design)
            .collect()
            .expect("facts pass should succeed");
        let errors = match DriverDrcChecker::new(&design, &facts).check_collect() {
            Ok(_) => panic!("overlapping tri-state enables must conflict"),
            Err(errors) => errors,
        };

        assert!(errors.iter().any(|error| matches!(
            error,
            CompileError::Lowering { kind, .. }
                if matches!(
                    kind.as_ref(),
                    LoweringError::Driver(DriverError::DuplicateHardwareDriver { name })
                        if name == "bus"
                )
        )));
    }

    #[test]
    fn duplicate_driver_keeps_expansion_call_stack_in_related_spans() {
        let source = SourceId::new(7);
        let call_span = Span::new_in(source, 40, 52);
        let previous_origin = EirOrigin::new(
            Span::new_in(source, 10, 16),
            vec![EirExpansion::new("DoubleDrive", "v", call_span)],
        );
        let conflicting_origin = EirOrigin::new(
            Span::new_in(source, 20, 26),
            vec![EirExpansion::new("DoubleDrive", "v", call_span)],
        );
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![
                EirItem::Signal {
                    width: "1".into(),
                    name: "v".to_string(),
                    activity: crate::eir::EirSignalActivity::Required,
                    origin: previous_origin.clone(),
                },
                EirItem::Drive {
                    lhs: EirPlace::Ident("v".to_string()),
                    rhs: EirExpr::Int(0),
                    reads: Vec::new(),
                    origin: previous_origin,
                },
                EirItem::Drive {
                    lhs: EirPlace::Ident("v".to_string()),
                    rhs: EirExpr::Int(1),
                    reads: Vec::new(),
                    origin: conflicting_origin,
                },
            ],
        );
        let design = match validated_design(vec![module]) {
            Ok(design) => design,
            Err(error) => panic!("test EIR should assemble: {error}"),
        };
        let facts = match crate::driver::DriverFactsCollector::new(&design).collect() {
            Ok(facts) => facts,
            Err(errors) => panic!("facts pass should succeed: {errors:?}"),
        };

        let errors = match DriverDrcChecker::new(&design, &facts).check_collect() {
            Ok(_) => panic!("duplicate driver must fail DRC"),
            Err(errors) => errors,
        };
        let diagnostic = errors
            .iter()
            .find_map(|error| match error {
                CompileError::Lowering { kind, diagnostic } => matches!(
                    kind.as_ref(),
                    LoweringError::Driver(DriverError::DuplicateHardwareDriver { name })
                        if name == "v"
                )
                .then_some(diagnostic.as_ref()),
                _ => None,
            })
            .expect("duplicate hardware driver diagnostic must be present");

        assert!(
            diagnostic
                .related
                .iter()
                .any(|related| related.span == call_span),
            "driver conflict diagnostics must include expansion callsite"
        );
        assert!(matches!(
            facts.drives().first().map(DriveFact::guard),
            Some(guard) if *guard == EirGuard::root()
        ));
    }

    #[test]
    fn continuous_drive_to_storage_is_rejected_defensively() {
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![
                EirItem::Storage {
                    width: "1".into(),
                    name: "state".to_string(),
                    origin: origin(),
                },
                EirItem::Drive {
                    lhs: EirPlace::Ident("state".to_string()),
                    rhs: EirExpr::Int(0),
                    reads: Vec::new(),
                    origin: origin(),
                },
            ],
        );
        let design = validated_design(vec![module]).expect("test EIR should assemble");
        let facts = crate::driver::DriverFactsCollector::new(&design)
            .collect()
            .expect("facts pass should succeed");
        let errors = match DriverDrcChecker::new(&design, &facts).check_collect() {
            Ok(_) => panic!("continuous drive to storage must fail DRC"),
            Err(errors) => errors,
        };

        assert!(errors.iter().any(|error| matches!(
            error,
            CompileError::Lowering { kind, .. }
                if matches!(
                    kind.as_ref(),
                    LoweringError::Driver(DriverError::ContinuousDriveTargetIsReg { name })
                        if name == "state"
                )
        )));
    }
}
