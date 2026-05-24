use crate::{
    driver_place::{DriverObjectTable, DriverPlace},
    eir::EirSignalActivity,
    eir_guard::EirGuard,
    eir_origin::EirOrigin,
};
use syl_hw::ObjectId;

mod activity;
mod bounds_check;
mod coverage;
mod drc;
mod facts;
mod guard;
mod guard_coverage;
mod loop_bounds;
mod summary;

use activity::DriverSignalActivityChecker;
use bounds_check::DriverBoundsChecker;
use coverage::{DriverCompletenessChecker, DriverReadCompletenessChecker};
pub(crate) use drc::{DriverDrcChecker, DriverDrcReport};
pub(crate) use facts::DriverFactsCollector;
use guard::DriverGuardSet;
use summary::CellSummaryCollector;

#[non_exhaustive]
pub(crate) struct DriverFacts {
    objects: DriverObjectTable,
    drives: Vec<DriveFact>,
    reads: Vec<ReadFact>,
    creates: Vec<CreateFact>,
    summary_cells: Vec<DriverCellSummary>,
}

impl DriverFacts {
    fn new(
        objects: DriverObjectTable,
        drives: Vec<DriveFact>,
        reads: Vec<ReadFact>,
        creates: Vec<CreateFact>,
        cell_summaries: Vec<DriverCellSummary>,
    ) -> Self {
        Self {
            objects,
            drives,
            reads,
            creates,
            summary_cells: cell_summaries,
        }
    }

    pub(crate) fn objects(&self) -> &DriverObjectTable {
        &self.objects
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
    effect: DriveEffect,
    guard: EirGuard,
    origin: EirOrigin,
}

impl DriveFact {
    fn new(
        module: impl Into<String>,
        target: DriverPlace,
        effect: DriveEffect,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            target,
            effect,
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

    pub(crate) fn effect(&self) -> &DriveEffect {
        &self.effect
    }

    pub(crate) fn guard(&self) -> &EirGuard {
        &self.guard
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum DriveEffect {
    Continuous,
    Next { storage_target: Box<DriverPlace> },
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
