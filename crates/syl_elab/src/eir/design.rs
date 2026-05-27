use std::sync::Arc;

use crate::eir::module::EirModule;
use crate::eir::signal::{EirDrive, EirObject, EirRead};

#[non_exhaustive]
pub(crate) struct EirRawDesign {
    modules: Vec<EirModule>,
}

impl EirRawDesign {
    pub(crate) fn new(modules: Vec<EirModule>) -> Self {
        Self { modules }
    }

    pub(crate) fn modules(&self) -> &[EirModule] {
        &self.modules
    }
}

#[non_exhaustive]
pub(crate) struct EirDesign {
    raw: Arc<EirRawDesign>,
    facts: Arc<EirDesignFacts>,
}

impl EirDesign {
    pub(crate) fn from_parts(raw: Arc<EirRawDesign>, facts: Arc<EirDesignFacts>) -> Self {
        Self { raw, facts }
    }

    pub(crate) fn modules(&self) -> &[EirModule] {
        self.raw.modules()
    }

    pub(crate) fn objects(&self) -> &[EirObject] {
        self.facts.objects()
    }

    pub(crate) fn drives(&self) -> &[EirDrive] {
        self.facts.drives()
    }

    pub(crate) fn reads(&self) -> &[EirRead] {
        self.facts.reads()
    }
}

#[non_exhaustive]
pub(crate) struct EirDesignFacts {
    objects: Vec<EirObject>,
    drives: Vec<EirDrive>,
    reads: Vec<EirRead>,
}

impl EirDesignFacts {
    pub(crate) fn new(
        objects: Vec<EirObject>,
        drives: Vec<EirDrive>,
        reads: Vec<EirRead>,
    ) -> Self {
        Self {
            objects,
            drives,
            reads,
        }
    }

    pub(crate) fn objects(&self) -> &[EirObject] {
        &self.objects
    }

    pub(crate) fn drives(&self) -> &[EirDrive] {
        &self.drives
    }

    pub(crate) fn reads(&self) -> &[EirRead] {
        &self.reads
    }
}
