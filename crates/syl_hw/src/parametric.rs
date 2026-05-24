use crate::{HwCellSummary, HwExpr, HwItem, HwOrigin, HwParam, HwPort};

#[non_exhaustive]
pub struct ParametricHwDesign {
    modules: Vec<ParametricHwModule>,
    driver_facts: Vec<crate::HwDriveFact>,
    read_facts: Vec<crate::HwReadFact>,
    create_facts: Vec<crate::HwCreateFact>,
    cell_summaries: Vec<HwCellSummary>,
}

impl ParametricHwDesign {
    pub fn new(
        modules: Vec<ParametricHwModule>,
        driver_facts: Vec<crate::HwDriveFact>,
        read_facts: Vec<crate::HwReadFact>,
        create_facts: Vec<crate::HwCreateFact>,
        cell_summaries: Vec<HwCellSummary>,
    ) -> Self {
        Self {
            modules,
            driver_facts,
            read_facts,
            create_facts,
            cell_summaries,
        }
    }

    pub fn debug_dump(&self) -> String {
        let modules = self
            .modules
            .iter()
            .map(|module| module.name().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "hwir modules={} driver_facts={} read_facts={} create_facts={} cell_summaries={} [{}]",
            self.modules.len(),
            self.driver_facts.len(),
            self.read_facts.len(),
            self.create_facts.len(),
            self.cell_summaries.len(),
            modules,
        )
    }

    pub fn modules(&self) -> &[ParametricHwModule] {
        &self.modules
    }

    pub fn driver_facts(&self) -> &[crate::HwDriveFact] {
        &self.driver_facts
    }

    pub fn read_facts(&self) -> &[crate::HwReadFact] {
        &self.read_facts
    }

    pub fn create_facts(&self) -> &[crate::HwCreateFact] {
        &self.create_facts
    }

    pub fn cell_summaries(&self) -> &[HwCellSummary] {
        &self.cell_summaries
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct ParametricHwModule {
    name: String,
    params: Vec<HwParam>,
    ports: Vec<HwPort>,
    items: Vec<ParametricHwItem>,
}

impl ParametricHwModule {
    pub fn new(
        name: impl Into<String>,
        params: Vec<HwParam>,
        ports: Vec<HwPort>,
        items: Vec<ParametricHwItem>,
    ) -> Self {
        Self {
            name: name.into(),
            params,
            ports,
            items,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn params(&self) -> &[HwParam] {
        &self.params
    }

    pub fn ports(&self) -> &[HwPort] {
        &self.ports
    }

    pub fn items(&self) -> &[ParametricHwItem] {
        &self.items
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub enum ParametricHwItem {
    Core {
        item: HwItem,
        origin: HwOrigin,
    },
    StaticIf {
        cond: HwExpr,
        label: String,
        then_items: Vec<ParametricHwItem>,
        else_items: Vec<ParametricHwItem>,
        origin: HwOrigin,
    },
    StaticFor {
        index: String,
        start: HwExpr,
        end: HwExpr,
        label: String,
        items: Vec<ParametricHwItem>,
        origin: HwOrigin,
    },
}

impl ParametricHwItem {
    pub fn core(item: HwItem, origin: HwOrigin) -> Self {
        Self::Core { item, origin }
    }

    pub fn origin(&self) -> &HwOrigin {
        match self {
            Self::Core { origin, .. }
            | Self::StaticIf { origin, .. }
            | Self::StaticFor { origin, .. } => origin,
        }
    }
}
