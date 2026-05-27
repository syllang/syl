use crate::{HwExpr, HwItem, HwOrigin, HwParam, HwPort};

#[derive(Debug)]
#[non_exhaustive]
pub struct ParametricHwDesign {
    modules: Vec<ParametricHwModule>,
}

impl ParametricHwDesign {
    pub fn new(modules: Vec<ParametricHwModule>) -> Self {
        Self { modules }
    }

    pub fn debug_dump(&self) -> String {
        let modules = self
            .modules
            .iter()
            .map(|module| module.name().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("hwir modules={} [{}]", self.modules.len(), modules,)
    }

    pub fn modules(&self) -> &[ParametricHwModule] {
        &self.modules
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct ParametricHwModule {
    doc: Option<String>,
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
            doc: None,
            name: name.into(),
            params,
            ports,
            items,
        }
    }

    pub fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }

    pub fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
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
