use crate::HwExpr;
use syl_span::SourceId;

/// Core (non-parametric) hardware design: an ordered collection of modules.
///
/// Lower-level cousin of [`ParametricHwDesign`](crate::ParametricHwDesign):
/// modules hold plain [`HwItem`]s without per-item [`HwOrigin`] or open
/// `StaticIf` / `StaticFor`. Prefer `ParametricHwDesign` as the elaboration →
/// backend exchange format.
///
/// # Main usage flow
///
/// ```text
///  (tests / specialized tooling constructing core HWIR)
///              |
///              v
///    +---------------------+
///    |      HwDesign       |   new(modules)
///    +----------+----------+
///               |
///               v
///          modules() / debug_dump()
///
///  Production pipeline path uses ParametricHwDesign instead:
///    elab --> ParametricHwDesign --> normalize --> SystemVerilogBackend
/// ```
#[non_exhaustive]
pub struct HwDesign {
    modules: Vec<HwModule>,
}

impl HwDesign {
    pub fn new(modules: Vec<HwModule>) -> Self {
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
        format!("hw_design modules={} [{}]", self.modules.len(), modules)
    }

    /// Returns all modules in this design.
    pub fn modules(&self) -> &[HwModule] {
        &self.modules
    }
}

/// Enable condition (guard) attached to a hardware signal or assignment.
///
/// Guards form a stack of scopes — each `if`/`else`/loop pushes a frame
/// that controls whether the enclosed hardware is active.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwGuard {
    frames: Vec<HwGuardFrame>,
}

impl HwGuard {
    pub fn new(frames: Vec<HwGuardFrame>) -> Self {
        Self { frames }
    }

    /// Returns the guard frames from innermost to outermost.
    pub fn frames(&self) -> &[HwGuardFrame] {
        &self.frames
    }

    /// Returns `true` if this guard is always active (no enclosing scope).
    pub fn is_root(&self) -> bool {
        self.frames.is_empty()
    }

    /// Formats the guard as a path string (e.g. `label:then/loop_label`).
    pub fn display(&self) -> String {
        if self.frames.is_empty() {
            return "root".to_string();
        }
        self.frames
            .iter()
            .map(HwGuardFrame::display)
            .collect::<Vec<_>>()
            .join("/")
    }
}

/// A single scope frame in a guard condition.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwGuardFrame {
    IfThen { label: String },
    IfElse { label: String },
    Loop { label: String },
}

impl HwGuardFrame {
    pub fn display(&self) -> String {
        match self {
            Self::IfThen { label } => format!("{label}:then"),
            Self::IfElse { label } => format!("{label}:else"),
            Self::Loop { label } => label.clone(),
        }
    }
}

/// Source location origin for an elaborated hardware object.
///
/// Tracks the original source span plus any expansion stack from
/// elaboration (which cell instantiations produced this object).
///
/// **Immutable sharing caveat:** the expansion stack is captured by value
/// when `HwOrigin` is constructed and is never extended afterward. Cloning
/// a `HwOrigin` for multiple hardware facts or summaries preserves the same
/// stack snapshot; it does not pick up later outer expansions added elsewhere
/// in the elaboration pipeline. Downstream code should treat the stack as the
/// exact instantiation path known at construction time, not as a live view.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwOrigin {
    source_field: Option<String>,
    source: SourceId,
    span_start: usize,
    span_end: usize,
    expansion_stack: Vec<HwExpansion>,
}

impl HwOrigin {
    /// Syl field before flattening an interface or prefixing an in-place local.
    pub fn source_field(&self) -> Option<&str> {
        self.source_field.as_deref()
    }

    pub fn with_source_field(mut self, field: Option<String>) -> Self {
        self.source_field = field;
        self
    }

    pub fn source_index(&self) -> usize {
        self.source.get()
    }
    pub fn new(
        source: SourceId,
        span_start: usize,
        span_end: usize,
        expansion_stack: Vec<HwExpansion>,
    ) -> Self {
        Self {
            source,
            source_field: None,
            span_start,
            span_end,
            expansion_stack,
        }
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    pub fn span_start(&self) -> usize {
        self.span_start
    }

    pub fn span_end(&self) -> usize {
        self.span_end
    }

    pub fn expansion_stack(&self) -> &[HwExpansion] {
        &self.expansion_stack
    }
}

/// A single level of elaboration expansion (which cell call produced this).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwExpansion {
    callable: String,
    instance: String,
    source: SourceId,
    span_start: usize,
    span_end: usize,
}

impl HwExpansion {
    pub fn new(
        callable: impl Into<String>,
        instance: impl Into<String>,
        source: SourceId,
        span_start: usize,
        span_end: usize,
    ) -> Self {
        Self {
            callable: callable.into(),
            instance: instance.into(),
            source,
            span_start,
            span_end,
        }
    }

    pub fn callable(&self) -> &str {
        &self.callable
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    pub fn span_start(&self) -> usize {
        self.span_start
    }

    pub fn span_end(&self) -> usize {
        self.span_end
    }
}

/// A single elaborated hardware module with parameters, ports, and internal items.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwModule {
    doc: Option<String>,
    name: String,
    params: Vec<HwParam>,
    ports: Vec<HwPort>,
    items: Vec<HwItem>,
}

impl HwModule {
    pub fn new(
        name: impl Into<String>,
        params: Vec<HwParam>,
        ports: Vec<HwPort>,
        items: Vec<HwItem>,
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

    /// Returns the module name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the module parameters.
    pub fn params(&self) -> &[HwParam] {
        &self.params
    }

    /// Returns the module ports.
    pub fn ports(&self) -> &[HwPort] {
        &self.ports
    }

    /// Returns the internal items (signals, instances, drives, etc.).
    pub fn items(&self) -> &[HwItem] {
        &self.items
    }
}

/// A parameter on an elaborated hardware module.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwParam {
    default_expr: Option<HwExpr>,
    is_bool: bool,
    doc: Option<String>,
    name: String,
    default: String,
}

impl HwParam {
    /// Source-level default, absent for a required generic. The SV printing
    /// default also permits standalone emission of unspecialized modules.
    pub fn default_expr(&self) -> Option<&HwExpr> {
        self.default_expr.as_ref()
    }
    pub fn with_default_expr(mut self, expr: Option<HwExpr>) -> Self {
        self.default_expr = expr;
        self
    }

    pub fn with_bool(mut self, is_bool: bool) -> Self {
        self.is_bool = is_bool;
        self
    }
    pub fn is_bool(&self) -> bool {
        self.is_bool
    }

    pub fn new(name: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            doc: None,
            name: name.into(),
            default: default.into(),
            is_bool: false,
            default_expr: None,
        }
    }

    pub fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }

    /// Returns the doc comment for this parameter, if any.
    pub fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
    }

    /// Returns the parameter name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the default value string.
    pub fn default(&self) -> &str {
        &self.default
    }
}

/// A port on an elaborated hardware module.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwPort {
    origin: Option<HwOrigin>,
    domain: crate::HwPortDomain,
    source_name: Option<String>,
    doc: Option<String>,
    direction: HwDirection,
    width: String,
    name: String,
}

impl HwPort {
    pub fn with_source(
        mut self,
        name: impl Into<String>,
        origin: HwOrigin,
        domain: crate::HwPortDomain,
    ) -> Self {
        self.source_name = Some(name.into());
        self.origin = Some(origin);
        self.domain = domain;
        self
    }

    pub fn origin(&self) -> Option<&HwOrigin> {
        self.origin.as_ref()
    }
    pub fn source_name(&self) -> &str {
        self.source_name.as_deref().unwrap_or(&self.name)
    }
    pub fn domain(&self) -> &crate::HwPortDomain {
        &self.domain
    }

    pub fn new(direction: HwDirection, width: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            doc: None,
            origin: None,
            domain: crate::HwPortDomain::Data,
            source_name: None,
            direction,
            width: width.into(),
            name: name.into(),
        }
    }

    pub fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }

    /// Returns the doc comment for this port, if any.
    pub fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
    }

    /// Returns the port direction.
    pub fn direction(&self) -> HwDirection {
        self.direction
    }

    /// Returns the port width expression (e.g. `"(N)-1:0"`).
    pub fn width(&self) -> &str {
        &self.width
    }

    /// Returns the port name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

/// Port direction for an elaborated hardware design.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwDirection {
    In,
    InOut,
    Out,
}

/// An item inside an elaborated hardware module body.
#[derive(Debug)]
#[non_exhaustive]
pub enum HwItem {
    /// A compile-time parameter assignment within the module.
    StaticParam { name: String, value: HwExpr },
    /// A combinational signal declaration.
    SignalDecl { width: String, name: String },
    /// A storage element (register) declaration.
    StorageDecl { width: String, name: String },
    /// A continuous assignment: `lhs = rhs`.
    ContinuousDrive { lhs: HwExpr, rhs: HwExpr },
    /// A clocked storage element with optional reset.
    ClockedStorage {
        clock: HwExpr,
        target: HwExpr,
        reset: Option<HwReset>,
        next: HwExpr,
    },
    /// A clocked runtime assertion emitted as backend verification logic.
    ClockedAssert {
        clock: HwExpr,
        trigger: HwExpr,
        message: HwExpr,
    },
    /// A sub-module instance.
    Instance(HwInstance),
    /// Conditional elaboration: `if (cond) then_items else else_items`.
    StaticIf {
        cond: HwExpr,
        label: String,
        then_items: Vec<HwItem>,
        else_items: Vec<HwItem>,
    },
    /// Replicated elaboration: `for index in start..end`.
    StaticFor {
        index: String,
        start: HwExpr,
        end: HwExpr,
        label: String,
        items: Vec<HwItem>,
    },
    /// An error marker — elaborating this item produces a compile error.
    InitialError { message: HwExpr },
    /// An unpacked memory array with a packed element width.
    MemoryDecl {
        name: String,
        elem_width: String,
        depth: String,
    },
    /// An enabled synchronous memory write port.
    MemoryWrite {
        clock: HwExpr,
        memory: String,
        enable: HwExpr,
        addr: HwExpr,
        data: HwExpr,
    },
    /// An enabled synchronous memory read port targeting declared storage.
    MemorySyncRead {
        clock: HwExpr,
        memory: String,
        enable: HwExpr,
        addr: HwExpr,
        target: HwExpr,
    },
}

/// Reset specification for a clocked storage element.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwReset {
    mode: crate::HwResetMode,
    polarity: crate::HwResetPolarity,
    condition: HwExpr,
    value: HwExpr,
}

impl HwReset {
    pub fn with_mode(mut self, mode: crate::HwResetMode) -> Self {
        self.mode = mode;
        self
    }
    pub fn with_polarity(mut self, polarity: crate::HwResetPolarity) -> Self {
        self.polarity = polarity;
        self
    }
    pub fn mode(&self) -> crate::HwResetMode {
        self.mode
    }
    pub fn polarity(&self) -> crate::HwResetPolarity {
        self.polarity
    }
    pub fn new(condition: HwExpr, value: HwExpr) -> Self {
        Self {
            condition,
            value,
            mode: Default::default(),
            polarity: Default::default(),
        }
    }

    pub fn condition(&self) -> &HwExpr {
        &self.condition
    }

    pub fn value(&self) -> &HwExpr {
        &self.value
    }
}

/// A sub-module instance within an elaborated module.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwInstance {
    source_name: Option<String>,
    target_canonical_path: Vec<String>,
    module: String,
    params: Vec<HwParamBind>,
    name: String,
    connections: Vec<HwConnection>,
}

impl HwInstance {
    pub fn new(
        module: impl Into<String>,
        params: Vec<HwParamBind>,
        name: impl Into<String>,
        connections: Vec<HwConnection>,
    ) -> Self {
        Self {
            target_canonical_path: Vec::new(),
            source_name: None,
            module: module.into(),
            params,
            name: name.into(),
            connections,
        }
    }

    pub fn with_source_name(mut self, name: impl Into<String>) -> Self {
        self.source_name = Some(name.into());
        self
    }

    pub fn source_name(&self) -> &str {
        self.source_name.as_deref().unwrap_or(&self.name)
    }

    /// Returns the instantiated module name.
    pub fn module(&self) -> &str {
        &self.module
    }

    /// Returns the parameter bindings for this instance.
    pub fn params(&self) -> &[HwParamBind] {
        &self.params
    }

    /// Returns the instance name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the port connections for this instance.
    pub fn connections(&self) -> &[HwConnection] {
        &self.connections
    }
    pub fn with_target_canonical_path(mut self, path: Vec<String>) -> Self {
        self.target_canonical_path = path;
        self
    }
    pub fn target_canonical_path(&self) -> &[String] {
        &self.target_canonical_path
    }
}

/// A parameter binding: `name = value` in an instance.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwParamBind {
    name: String,
    value: String,
}

impl HwParamBind {
    pub fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    /// Returns the parameter name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parameter value string.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// A port connection on a module instance: `formal = actual`.
#[derive(Debug)]
#[non_exhaustive]
pub struct HwConnection {
    formal: String,
    actual: HwExpr,
}

impl HwConnection {
    pub fn new(formal: impl Into<String>, actual: HwExpr) -> Self {
        Self {
            formal: formal.into(),
            actual,
        }
    }

    /// Returns the formal port name.
    pub fn formal(&self) -> &str {
        &self.formal
    }

    /// Returns the actual expression connected to this port.
    pub fn actual(&self) -> &HwExpr {
        &self.actual
    }
}
