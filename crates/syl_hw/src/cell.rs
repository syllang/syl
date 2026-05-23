#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwCellSummary {
    callable: String,
    instance: String,
    drives: Vec<crate::HwPlace>,
    reads: Vec<crate::HwPlace>,
    creates: Vec<String>,
    origin: crate::HwOrigin,
}

impl HwCellSummary {
    pub fn builder(
        callable: impl Into<String>,
        instance: impl Into<String>,
        origin: crate::HwOrigin,
    ) -> HwCellSummaryBuilder {
        HwCellSummaryBuilder {
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

    pub fn drives(&self) -> &[crate::HwPlace] {
        &self.drives
    }

    pub fn reads(&self) -> &[crate::HwPlace] {
        &self.reads
    }

    pub fn creates(&self) -> &[String] {
        &self.creates
    }

    pub fn origin(&self) -> &crate::HwOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwCellSummaryBuilder {
    callable: String,
    instance: String,
    drives: Vec<crate::HwPlace>,
    reads: Vec<crate::HwPlace>,
    creates: Vec<String>,
    origin: crate::HwOrigin,
}

impl HwCellSummaryBuilder {
    pub fn drives(mut self, drives: Vec<crate::HwPlace>) -> Self {
        self.drives = drives;
        self
    }

    pub fn reads(mut self, reads: Vec<crate::HwPlace>) -> Self {
        self.reads = reads;
        self
    }

    pub fn creates(mut self, creates: Vec<String>) -> Self {
        self.creates = creates;
        self
    }

    pub fn build(self) -> HwCellSummary {
        HwCellSummary {
            callable: self.callable,
            instance: self.instance,
            drives: self.drives,
            reads: self.reads,
            creates: self.creates,
            origin: self.origin,
        }
    }
}
