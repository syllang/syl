use crate::{HwExpr, HwItem, HwOrigin, HwParam, HwPort};

/// Backend-facing hardware IR root produced by elaboration.
///
/// Structurally similar to [`HwDesign`](crate::HwDesign) (an ordered list of
/// modules), but each item is a [`ParametricHwItem`] that:
/// - keeps a per-item [`HwOrigin`] — the source span of the construct that
///   produced it, plus the elaboration expansion stack (which cell
///   instantiations nested to create it); and
/// - can still contain open static structure (`StaticIf` / `StaticFor`) rather
///   than only fully flattened core hardware items.
///
/// # Main usage flow
///
/// ```text
///  HardwareCompiler::compile_tir / ElaborationOutput::hwir
///              |
///              v
///    +---------------------+
///    | ParametricHwDesign  |   modules: [ParametricHwModule]
///    +----------+----------+
///               |
///     +---------+------------------+
///     |                            |
///     v                            v
///  HwValidator::validate    HwNormalizer::normalize
///  Ok(()) | Report          NormalizedParametricHwDesign
///                                      |
///                                      v
///                         SystemVerilogBackend::emit / debug_dump
///                                      |
///                                      v
///                              SystemVerilog text
/// ```
///
/// Emitters should normalize/validate before lowering; construction via
/// [`ParametricHwDesign::new`] is also used in unit tests that build HWIR
/// directly.
#[derive(Debug)]
#[non_exhaustive]
pub struct ParametricHwDesign {
    modules: Vec<ParametricHwModule>,
}

impl ParametricHwDesign {
    pub fn new(modules: Vec<ParametricHwModule>) -> Self {
        Self { modules }
    }

    /// Returns a summary string for debugging.
    pub fn debug_dump(&self) -> String {
        let modules = self
            .modules
            .iter()
            .map(|module| module.name().to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("hwir modules={} [{}]", self.modules.len(), modules,)
    }

    /// Returns all modules in this design.
    pub fn modules(&self) -> &[ParametricHwModule] {
        &self.modules
    }
}

/// A module in a [`ParametricHwDesign`]: name, params, ports, and items.
///
/// Unlike [`HwModule`](crate::HwModule), items are [`ParametricHwItem`]s, each
/// carrying its own [`HwOrigin`] and optional static if/for structure.
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

/// One item inside a [`ParametricHwModule`].
///
/// Every variant carries an [`HwOrigin`]:
/// - **span** — source file and byte range of the Syl construct that produced
///   this item; and
/// - **expansion stack** — nested cell instantiations (callable + instance
///   names and their spans) that were active when elaboration created it.
///
/// That origin is a construction-time snapshot (see [`HwOrigin`]); it is not
/// updated if later outer expansions appear elsewhere in the pipeline.
///
/// Variants also retain static elaboration structure that has not yet been
/// fully expanded into core hardware:
#[derive(Debug)]
#[non_exhaustive]
pub enum ParametricHwItem {
    /// Core hardware node ([`HwItem`]) plus the origin of that node.
    Core { item: HwItem, origin: HwOrigin },
    /// Compile-time conditional: `if (cond) { then_items } else { else_items }`.
    StaticIf {
        cond: HwExpr,
        label: String,
        then_items: Vec<ParametricHwItem>,
        else_items: Vec<ParametricHwItem>,
        origin: HwOrigin,
    },
    /// Compile-time replication: `for index in start..end { items }`.
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
