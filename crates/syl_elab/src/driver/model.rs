use crate::{
    driver::place::{DriverObjectTable, DriverPlace},
    eir::{EirExpr, EirGuard, EirOrigin, EirSignalActivity},
};
use syl_hw::ObjectId;

#[non_exhaustive]
pub(crate) struct DriverFacts {
    objects: DriverObjectTable,
    drives: Vec<DriveFact>,
    reads: Vec<ReadFact>,
    creates: Vec<CreateFact>,
    summary_cells: Vec<DriverCellSummary>,
}

impl DriverFacts {
    pub(crate) fn new(
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
    value: Option<EirExpr>,
    guard: EirGuard,
    origin: EirOrigin,
}

impl DriveFact {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        module: impl Into<String>,
        target: DriverPlace,
        effect: DriveEffect,
        value: Option<EirExpr>,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            target,
            effect,
            value,
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

    pub(crate) fn value(&self) -> Option<&EirExpr> {
        self.value.as_ref()
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

impl CreateFact {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        module: impl Into<String>,
        name: impl Into<String>,
        object_id: ObjectId,
        kind: CreateKind,
        activity: EirSignalActivity,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            name: name.into(),
            object_id,
            kind,
            activity,
            origin,
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
    pub(crate) fn new(
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

// Driver-local cell summary shape used by the current HWIR lowering adapter. Keep this isolated so
// the new first-class `CellSummary` model can evolve independently.
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
    pub(crate) fn new(
        callable: impl Into<String>,
        instance: impl Into<String>,
        origin: EirOrigin,
    ) -> Self {
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

    pub(crate) fn add_drive(&mut self, place: DriverPlace) {
        if !self.drives.contains(&place) {
            self.drives.push(place);
        }
    }

    pub(crate) fn add_read(&mut self, place: DriverPlace) {
        if !self.reads.contains(&place) {
            self.reads.push(place);
        }
    }

    pub(crate) fn add_create(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.creates.contains(&name) {
            self.creates.push(name);
        }
    }
}
