use derive_builder::Builder;
use syl_hw::{HwGuard, HwOrigin, HwPlace, ObjectId};
use syl_sema::OpaqueSummaryTable;

/// Metadata collected during elaboration about hardware drivers,
/// reads, creates, and cell summaries.
///
/// Attached to elaborated output for downstream analysis (DRC, coverage, etc.).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareMetadata {
    driver_facts: Vec<HardwareDriveFact>,
    read_facts: Vec<HardwareReadFact>,
    create_facts: Vec<HardwareCreateFact>,
    cell_summaries: Vec<HardwareCellSummary>,
    opaque_summaries: OpaqueSummaryTable,
}

impl HardwareMetadata {
    pub(crate) fn new(
        driver_facts: Vec<HardwareDriveFact>,
        read_facts: Vec<HardwareReadFact>,
        create_facts: Vec<HardwareCreateFact>,
        cell_summaries: Vec<HardwareCellSummary>,
        opaque_summaries: OpaqueSummaryTable,
    ) -> Self {
        Self {
            driver_facts,
            read_facts,
            create_facts,
            cell_summaries,
            opaque_summaries,
        }
    }

    /// Returns a summary string for debugging.
    pub fn debug_dump(&self) -> String {
        format!(
            "hw_metadata driver_facts={} read_facts={} create_facts={} cell_summaries={} opaque_summaries={}",
            self.driver_facts.len(),
            self.read_facts.len(),
            self.create_facts.len(),
            self.cell_summaries.len(),
            self.opaque_summaries.len(),
        )
    }

    /// Returns all drive facts collected during elaboration.
    pub fn driver_facts(&self) -> &[HardwareDriveFact] {
        &self.driver_facts
    }

    /// Returns all read facts collected during elaboration.
    pub fn read_facts(&self) -> &[HardwareReadFact] {
        &self.read_facts
    }

    /// Returns all create facts collected during elaboration.
    pub fn create_facts(&self) -> &[HardwareCreateFact] {
        &self.create_facts
    }

    /// Returns all cell summaries collected during elaboration.
    pub fn cell_summaries(&self) -> &[HardwareCellSummary] {
        &self.cell_summaries
    }

    /// Returns the opaque summary table for external cell definitions.
    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        &self.opaque_summaries
    }
}

/// Records a signal drive event during elaboration - which module drives which signal.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareDriveFact {
    module: String,
    target: HwPlace,
    target_text: String,
    guard: HwGuard,
    guard_text: String,
    origin: HwOrigin,
}

impl HardwareDriveFact {
    pub(crate) fn new(
        module: impl Into<String>,
        target: HwPlace,
        guard: HwGuard,
        origin: HwOrigin,
    ) -> Self {
        let target_text = target.display();
        let guard_text = guard.display();
        Self {
            module: module.into(),
            target,
            target_text,
            guard,
            guard_text,
            origin,
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn target(&self) -> &str {
        &self.target_text
    }

    pub fn target_place(&self) -> &HwPlace {
        &self.target
    }

    pub fn guard(&self) -> &str {
        &self.guard_text
    }

    pub fn guard_model(&self) -> &HwGuard {
        &self.guard
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

/// Records a signal read event during elaboration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareReadFact {
    module: String,
    source: HwPlace,
    source_text: String,
    guard: HwGuard,
    guard_text: String,
    origin: HwOrigin,
}

impl HardwareReadFact {
    pub(crate) fn new(
        module: impl Into<String>,
        source: HwPlace,
        guard: HwGuard,
        origin: HwOrigin,
    ) -> Self {
        let source_text = source.display();
        let guard_text = guard.display();
        Self {
            module: module.into(),
            source,
            source_text,
            guard,
            guard_text,
            origin,
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn source(&self) -> &str {
        &self.source_text
    }

    pub fn source_place(&self) -> &HwPlace {
        &self.source
    }

    pub fn guard(&self) -> &str {
        &self.guard_text
    }

    pub fn guard_model(&self) -> &HwGuard {
        &self.guard
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

/// What kind of object was created during elaboration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HardwareCreateKind {
    Signal,
    Storage,
}

/// Records the creation of a hardware object during elaboration.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareCreateFact {
    module: String,
    name: String,
    object_id: ObjectId,
    kind: HardwareCreateKind,
    origin: HwOrigin,
}

impl HardwareCreateFact {
    pub(crate) fn new(
        module: impl Into<String>,
        name: impl Into<String>,
        object_id: ObjectId,
        kind: HardwareCreateKind,
        origin: HwOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            name: name.into(),
            object_id,
            kind,
            origin,
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> HardwareCreateKind {
        self.kind
    }

    pub fn object_id(&self) -> ObjectId {
        self.object_id
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

/// Summary of a cell instance's hardware behavior during elaboration.
///
/// Records what signals the cell drives, reads, and creates.
#[derive(Clone, Debug, PartialEq, Eq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct HardwareCellSummary {
    #[builder(setter(into))]
    callable: String,
    #[builder(setter(into))]
    instance: String,
    #[builder(default)]
    drives: Vec<HwPlace>,
    #[builder(default)]
    reads: Vec<HwPlace>,
    #[builder(default)]
    creates: Vec<String>,
    origin: HwOrigin,
}

impl HardwareCellSummary {
    pub fn builder(
        callable: impl Into<String>,
        instance: impl Into<String>,
        origin: HwOrigin,
    ) -> HardwareCellSummaryBuilder {
        HardwareCellSummaryBuilder::default()
            .callable(callable.into())
            .instance(instance.into())
            .origin(origin)
    }

    /// Returns the callable name (cell type).
    pub fn callable(&self) -> &str {
        &self.callable
    }

    /// Returns the instance name.
    pub fn instance(&self) -> &str {
        &self.instance
    }

    /// Returns the signals driven by this cell instance.
    pub fn drives(&self) -> &[HwPlace] {
        &self.drives
    }

    /// Returns the signals read by this cell instance.
    pub fn reads(&self) -> &[HwPlace] {
        &self.reads
    }

    /// Returns the names of objects created by this cell instance.
    pub fn creates(&self) -> &[String] {
        &self.creates
    }

    /// Returns the source origin of this cell summary.
    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

impl HardwareCellSummaryBuilder {
    pub fn build(self) -> HardwareCellSummary {
        self.try_build().expect(
            "HardwareCellSummaryBuilder must be initialized with callable, instance, and origin",
        )
    }
}
