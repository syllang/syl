use crate::{
    CompileError, DriverError,
    driver_place::{
        DriverExpr, DriverObjectTable, DriverPlace, DriverPlaceError, DriverPlaceResolver,
    },
    eir::{EirDesign, EirDriveKind, EirObjectKind, EirSignalActivity},
    eir_guard::EirGuard,
    eir_origin::EirOrigin,
};
use std::collections::BTreeSet;
use syl_hw::ObjectId;

mod activity;
mod bounds_check;
mod coverage;
mod guard;
mod guard_coverage;
mod loop_bounds;
mod summary;

use activity::DriverSignalActivityChecker;
use bounds_check::DriverBoundsChecker;
use coverage::{DriverCompletenessChecker, DriverReadCompletenessChecker};
use guard::DriverGuardSet;
use summary::CellSummaryCollector;

#[non_exhaustive]
pub(crate) struct DriverFacts {
    drives: Vec<DriveFact>,
    reads: Vec<ReadFact>,
    creates: Vec<CreateFact>,
    summary_cells: Vec<DriverCellSummary>,
}

impl DriverFacts {
    fn new(
        drives: Vec<DriveFact>,
        reads: Vec<ReadFact>,
        creates: Vec<CreateFact>,
        cell_summaries: Vec<DriverCellSummary>,
    ) -> Self {
        Self {
            drives,
            reads,
            creates,
            summary_cells: cell_summaries,
        }
    }

    pub(crate) fn drives(&self) -> &[DriveFact] {
        &self.drives
    }

    pub(crate) fn reads(&self) -> &[ReadFact] {
        &self.reads
    }

    pub(crate) fn creates(&self) -> &[CreateFact] {
        &self.creates
    }

    pub(crate) fn summary_cells(&self) -> &[DriverCellSummary] {
        &self.summary_cells
    }
}

#[non_exhaustive]
pub(crate) struct DriveFact {
    module: String,
    target: DriverPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl DriveFact {
    fn new(
        module: impl Into<String>,
        target: DriverPlace,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            target,
            guard,
            origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn target_place(&self) -> &DriverPlace {
        &self.target
    }

    pub(crate) fn guard(&self) -> &EirGuard {
        &self.guard
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[non_exhaustive]
pub(crate) struct CreateFact {
    module: String,
    name: String,
    object_id: ObjectId,
    kind: CreateKind,
    activity: EirSignalActivity,
    origin: EirOrigin,
}

struct CreateFactInput {
    module: String,
    name: String,
    object_id: ObjectId,
    kind: CreateKind,
    activity: EirSignalActivity,
    origin: EirOrigin,
}

impl CreateFact {
    fn new(input: CreateFactInput) -> Self {
        Self {
            module: input.module,
            name: input.name,
            object_id: input.object_id,
            kind: input.kind,
            activity: input.activity,
            origin: input.origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn kind(&self) -> CreateKind {
        self.kind
    }

    pub(crate) fn object_id(&self) -> ObjectId {
        self.object_id
    }

    pub(crate) fn activity(&self) -> EirSignalActivity {
        self.activity
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[non_exhaustive]
pub(crate) struct ReadFact {
    module: String,
    source: DriverPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl ReadFact {
    fn new(
        module: impl Into<String>,
        source: DriverPlace,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            source,
            guard,
            origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn source_place(&self) -> &DriverPlace {
        &self.source
    }

    pub(crate) fn guard(&self) -> &EirGuard {
        &self.guard
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(crate) enum CreateKind {
    Signal,
    Storage,
}

// Legacy driver-local cell summary shape used by the existing HWIR lowerer. Keep this isolated so the
// new first-class `CellSummary` model can evolve independently.
#[non_exhaustive]
pub(crate) struct DriverCellSummary {
    callable: String,
    instance: String,
    drives: Vec<DriverPlace>,
    reads: Vec<DriverPlace>,
    creates: Vec<String>,
    origin: EirOrigin,
}

impl DriverCellSummary {
    fn new(callable: impl Into<String>, instance: impl Into<String>, origin: EirOrigin) -> Self {
        Self {
            callable: callable.into(),
            instance: instance.into(),
            drives: Vec::new(),
            reads: Vec::new(),
            creates: Vec::new(),
            origin,
        }
    }

    pub(crate) fn callable(&self) -> &str {
        &self.callable
    }

    pub(crate) fn instance(&self) -> &str {
        &self.instance
    }

    pub(crate) fn drives(&self) -> &[DriverPlace] {
        &self.drives
    }

    pub(crate) fn reads(&self) -> &[DriverPlace] {
        &self.reads
    }

    pub(crate) fn creates(&self) -> &[String] {
        &self.creates
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }

    fn add_drive(&mut self, place: DriverPlace) {
        if !self.drives.contains(&place) {
            self.drives.push(place);
        }
    }

    fn add_read(&mut self, place: DriverPlace) {
        if !self.reads.contains(&place) {
            self.reads.push(place);
        }
    }

    fn add_create(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.creates.contains(&name) {
            self.creates.push(name);
        }
    }
}

#[non_exhaustive]
pub(crate) struct DriverAnalyzer<'a> {
    eir: &'a EirDesign,
    objects: DriverObjectTable,
    regs: BTreeSet<(String, String)>,
    nexts: Vec<NextClaim>,
    drive_claims: Vec<DriveClaim>,
    drives: Vec<DriveFact>,
    reads: Vec<ReadFact>,
    creates: Vec<CreateFact>,
    errors: Vec<CompileError>,
}

impl<'a> DriverAnalyzer<'a> {
    pub(crate) fn new(eir: &'a EirDesign) -> Self {
        Self {
            eir,
            objects: DriverObjectTable::new(),
            regs: BTreeSet::new(),
            nexts: Vec::new(),
            drive_claims: Vec::new(),
            drives: Vec::new(),
            reads: Vec::new(),
            creates: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(crate) fn analyze(self) -> Result<DriverFacts, CompileError> {
        self.analyze_collect()
            .map_err(|mut errors| errors.remove(0))
    }

    pub(crate) fn analyze_collect(mut self) -> Result<DriverFacts, Vec<CompileError>> {
        self.begin_analysis();
        self.index_objects();
        for object in self.eir.objects() {
            match object.kind() {
                EirObjectKind::Signal => {
                    let object_id = self.objects.intern(object.module(), object.name());
                    self.creates.push(CreateFact::new(CreateFactInput {
                        module: object.module().to_string(),
                        name: object.name().to_string(),
                        object_id,
                        kind: CreateKind::Signal,
                        activity: object.activity(),
                        origin: object.origin().clone(),
                    }));
                }
                EirObjectKind::Storage => {
                    self.regs
                        .insert((object.module().to_string(), object.name().to_string()));
                    let object_id = self.objects.intern(object.module(), object.name());
                    self.creates.push(CreateFact::new(CreateFactInput {
                        module: object.module().to_string(),
                        name: object.name().to_string(),
                        object_id,
                        kind: CreateKind::Storage,
                        activity: object.activity(),
                        origin: object.origin().clone(),
                    }));
                }
            }
        }
        for drive in self.eir.drives() {
            let target = match self
                .place_resolver(drive.module())
                .resolve_place(drive.target_place())
            {
                Ok(target) => target,
                Err(error) => {
                    self.errors
                        .push(self.place_error(error, drive.origin().span()));
                    continue;
                }
            };
            match drive.kind() {
                EirDriveKind::Continuous => {
                    if let Err(error) = self.record_drive(
                        drive.module(),
                        target,
                        drive.guard(),
                        drive.origin().clone(),
                    ) {
                        self.errors.push(error);
                    }
                }
                EirDriveKind::Next => {
                    if let Err(error) = self.record_next(
                        drive.module(),
                        target,
                        drive.guard(),
                        drive.origin().clone(),
                    ) {
                        self.errors.push(error);
                    }
                }
            }
        }
        for read in self.eir.reads() {
            let source = match self
                .place_resolver(read.module())
                .resolve_place(read.source_place())
            {
                Ok(source) => source,
                Err(error) => {
                    self.errors
                        .push(self.place_error(error, read.origin().span()));
                    continue;
                }
            };
            if let Err(error) = DriverBoundsChecker::new(&self.objects).check_place(
                &source,
                read.guard(),
                read.origin(),
            ) {
                self.errors.push(error);
                continue;
            }
            self.reads.push(ReadFact::new(
                read.module(),
                source,
                read.guard().clone(),
                read.origin().clone(),
            ));
        }
        self.collect_out_completeness();
        self.collect_read_completeness();
        self.collect_signal_activity();
        if !self.errors.is_empty() {
            return Err(self.errors);
        }
        let cell_summaries = CellSummaryCollector::new(&self.drives, &self.reads, &self.creates)
            .collect()
            .finish();
        Ok(DriverFacts::new(
            self.drives,
            self.reads,
            self.creates,
            cell_summaries,
        ))
    }

    fn begin_analysis(&mut self) {
        self.regs.clear();
        self.nexts.clear();
        self.drive_claims.clear();
        self.errors.clear();
    }

    fn index_objects(&mut self) {
        self.objects = DriverObjectTable::new();
        for module in self.eir.modules() {
            for port in module.ports() {
                self.objects
                    .intern_with_bound(module.name(), port.name(), port.width_bound());
            }
        }
        for object in self.eir.objects() {
            self.objects
                .intern_with_bound(object.module(), object.name(), object.width_bound());
        }
    }

    fn place_resolver<'module, 'objects>(
        &'objects self,
        module: &'module str,
    ) -> DriverPlaceResolver<'module, 'objects> {
        DriverPlaceResolver::new(module, &self.objects)
    }

    fn place_error(&self, error: DriverPlaceError, span: syl_span::Span) -> CompileError {
        let kind = match error {
            DriverPlaceError::UnsupportedExpr => DriverError::UnsupportedHardwareValueExpression,
            DriverPlaceError::UnknownObject { module, name } => {
                DriverError::UnknownHardwareObject { module, name }
            }
        };
        CompileError::driver_error(kind, span)
    }

    fn record_next(
        &mut self,
        module: &str,
        target: DriverPlace,
        guard: &EirGuard,
        origin: EirOrigin,
    ) -> Result<(), CompileError> {
        let name = target.display();
        let reg_key = (module.to_string(), name.clone());
        if !self.regs.contains(&reg_key) {
            return Err(CompileError::driver_error(
                DriverError::NextTargetIsNotReg { name: name.clone() },
                origin.span(),
            ));
        }
        if let Some(claim) = self
            .nexts
            .iter()
            .find(|claim| claim.matches(module, &target, guard))
        {
            return Err(CompileError::driver_error_with_related(
                DriverError::DuplicateNextDriver { name: name.clone() },
                origin.span(),
                [
                    (
                        claim.origin().span(),
                        "previous next driver claim".to_string(),
                    ),
                    (origin.span(), "conflicting next driver claim".to_string()),
                ],
            ));
        }
        self.nexts.push(NextClaim::new(
            module,
            target.clone(),
            guard.clone(),
            origin.clone(),
        ));
        self.record_drive(
            module,
            DriverPlace::Expr(DriverExpr::Ident(format!("{}.next", target.display()))),
            guard,
            origin,
        )
    }

    fn record_drive(
        &mut self,
        module: &str,
        target: DriverPlace,
        guard: &EirGuard,
        origin: EirOrigin,
    ) -> Result<(), CompileError> {
        DriverBoundsChecker::new(&self.objects).check_place(&target, guard, &origin)?;
        for claim in &self.drive_claims {
            if claim.module() != module || !claim.target().overlaps(&target) {
                continue;
            }
            if !DriverGuardSet::new(claim.guard(), guard).is_mutually_exclusive() {
                return Err(CompileError::driver_error_with_related(
                    DriverError::DuplicateHardwareDriver {
                        name: target.display(),
                    },
                    origin.span(),
                    [
                        (claim.origin().span(), "previous driver claim".to_string()),
                        (origin.span(), "conflicting driver claim".to_string()),
                    ],
                ));
            }
        }
        self.drive_claims.push(DriveClaim::new(
            module,
            target.clone(),
            guard.clone(),
            origin.clone(),
        ));
        self.drives
            .push(DriveFact::new(module, target, guard.clone(), origin));
        Ok(())
    }

    fn collect_out_completeness(&mut self) {
        let checker = DriverCompletenessChecker::new(self.eir, &self.objects, &self.drives);
        self.errors.extend(checker.collect_errors());
    }

    fn collect_read_completeness(&mut self) {
        let checker = DriverReadCompletenessChecker::new(
            &self.objects,
            &self.drives,
            &self.reads,
            &self.creates,
        );
        self.errors.extend(checker.collect_errors());
    }

    fn collect_signal_activity(&mut self) {
        self.errors.extend(
            DriverSignalActivityChecker::new(&self.creates, &self.drives, &self.objects)
                .collect_errors(),
        );
    }
}

struct DriveClaim {
    module: String,
    target: DriverPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl DriveClaim {
    fn new(
        module: impl Into<String>,
        target: DriverPlace,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            target,
            guard,
            origin,
        }
    }

    fn module(&self) -> &str {
        &self.module
    }

    fn target(&self) -> &DriverPlace {
        &self.target
    }

    fn guard(&self) -> &EirGuard {
        &self.guard
    }

    fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

struct NextClaim {
    module: String,
    target: DriverPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl NextClaim {
    fn new(
        module: impl Into<String>,
        target: DriverPlace,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            target,
            guard,
            origin,
        }
    }

    fn matches(&self, module: &str, target: &DriverPlace, guard: &EirGuard) -> bool {
        self.module == module && self.target == *target && self.guard == *guard
    }

    fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        LoweringError,
        eir::{EirDesignAssembler, EirItem, EirModule},
        eir_expr::EirExpr,
        eir_origin::EirOrigin,
        eir_place::EirPlace,
    };
    use syl_span::{SourceId, Span};

    #[test]
    fn rejects_unresolved_drive_root() {
        let span = Span::new_in(SourceId::new(0), 4, 12);
        let origin = EirOrigin::new(span, Vec::new());
        let module = EirModule::new(
            "Top",
            Vec::new(),
            Vec::new(),
            vec![EirItem::Drive {
                lhs: EirPlace::Ident("missing".to_string()),
                rhs: EirExpr::Int(0),
                reads: Vec::new(),
                origin,
            }],
        );
        let design = match EirDesignAssembler::assemble(vec![module]) {
            Ok(design) => design,
            Err(error) => panic!("test EIR should pass structural validation: {error}"),
        };

        let errors = match DriverAnalyzer::new(&design).analyze_collect() {
            Ok(_) => panic!("driver analyzer must reject unresolved drive roots"),
            Err(errors) => errors,
        };

        assert!(matches!(
            errors.first(),
            Some(CompileError::Lowering { kind, .. })
                if matches!(
                    kind.as_ref(),
                    LoweringError::Driver(DriverError::UnknownHardwareObject { module, name })
                        if module == "Top" && name == "missing"
                )
        ));
        assert_eq!(errors[0].diagnostic().span, span);
    }
}
