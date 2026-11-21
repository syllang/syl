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
mod tests;
