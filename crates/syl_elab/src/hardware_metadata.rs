use syl_hw::{HwGuard, HwOrigin, HwPlace, ObjectId};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareMetadata {
    driver_facts: Vec<HardwareDriveFact>,
    read_facts: Vec<HardwareReadFact>,
    create_facts: Vec<HardwareCreateFact>,
    cell_summaries: Vec<HardwareCellSummary>,
}

impl HardwareMetadata {
    pub(crate) fn new(
        driver_facts: Vec<HardwareDriveFact>,
        read_facts: Vec<HardwareReadFact>,
        create_facts: Vec<HardwareCreateFact>,
        cell_summaries: Vec<HardwareCellSummary>,
    ) -> Self {
        Self {
            driver_facts,
            read_facts,
            create_facts,
            cell_summaries,
        }
    }

    pub fn debug_dump(&self) -> String {
        format!(
            "hw_metadata driver_facts={} read_facts={} create_facts={} cell_summaries={}",
            self.driver_facts.len(),
            self.read_facts.len(),
            self.create_facts.len(),
            self.cell_summaries.len(),
        )
    }

    pub fn driver_facts(&self) -> &[HardwareDriveFact] {
        &self.driver_facts
    }

    pub fn read_facts(&self) -> &[HardwareReadFact] {
        &self.read_facts
    }

    pub fn create_facts(&self) -> &[HardwareCreateFact] {
        &self.create_facts
    }

    pub fn cell_summaries(&self) -> &[HardwareCellSummary] {
        &self.cell_summaries
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HardwareCreateKind {
    Signal,
    Storage,
}

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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareCellSummary {
    callable: String,
    instance: String,
    drives: Vec<HwPlace>,
    reads: Vec<HwPlace>,
    creates: Vec<String>,
    origin: HwOrigin,
}

impl HardwareCellSummary {
    pub fn builder(
        callable: impl Into<String>,
        instance: impl Into<String>,
        origin: HwOrigin,
    ) -> HardwareCellSummaryBuilder {
        HardwareCellSummaryBuilder {
            callable: callable.into(),
            instance: instance.into(),
            drives: Vec::new(),
            reads: Vec::new(),
            creates: Vec::new(),
            origin,
        }
    }

    pub fn callable(&self) -> &str {
        &self.callable
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    pub fn drives(&self) -> &[HwPlace] {
        &self.drives
    }

    pub fn reads(&self) -> &[HwPlace] {
        &self.reads
    }

    pub fn creates(&self) -> &[String] {
        &self.creates
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HardwareCellSummaryBuilder {
    callable: String,
    instance: String,
    drives: Vec<HwPlace>,
    reads: Vec<HwPlace>,
    creates: Vec<String>,
    origin: HwOrigin,
}

impl HardwareCellSummaryBuilder {
    pub fn drives(mut self, drives: Vec<HwPlace>) -> Self {
        self.drives = drives;
        self
    }

    pub fn reads(mut self, reads: Vec<HwPlace>) -> Self {
        self.reads = reads;
        self
    }

    pub fn creates(mut self, creates: Vec<String>) -> Self {
        self.creates = creates;
        self
    }

    pub fn build(self) -> HardwareCellSummary {
        HardwareCellSummary {
            callable: self.callable,
            instance: self.instance,
            drives: self.drives,
            reads: self.reads,
            creates: self.creates,
            origin: self.origin,
        }
    }
}
