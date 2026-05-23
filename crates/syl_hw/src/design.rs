use crate::{HwCellSummary, HwExpr, HwPlace, ObjectId};
use syl_span::SourceId;

#[non_exhaustive]
pub struct HwDesign {
    modules: Vec<HwModule>,
    driver_facts: Vec<HwDriveFact>,
    read_facts: Vec<HwReadFact>,
    create_facts: Vec<HwCreateFact>,
    cell_summaries: Vec<HwCellSummary>,
}

impl HwDesign {
    pub fn new(
        modules: Vec<HwModule>,
        driver_facts: Vec<HwDriveFact>,
        read_facts: Vec<HwReadFact>,
        create_facts: Vec<HwCreateFact>,
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

    pub fn modules(&self) -> &[HwModule] {
        &self.modules
    }

    pub fn driver_facts(&self) -> &[HwDriveFact] {
        &self.driver_facts
    }

    pub fn read_facts(&self) -> &[HwReadFact] {
        &self.read_facts
    }

    pub fn create_facts(&self) -> &[HwCreateFact] {
        &self.create_facts
    }

    pub fn cell_summaries(&self) -> &[HwCellSummary] {
        &self.cell_summaries
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HwDriveFact {
    module: String,
    target: HwPlace,
    target_text: String,
    guard: HwGuard,
    guard_text: String,
    origin: HwOrigin,
}

impl HwDriveFact {
    pub fn new(
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

#[derive(Debug)]
#[non_exhaustive]
pub struct HwReadFact {
    module: String,
    source: HwPlace,
    source_text: String,
    guard: HwGuard,
    guard_text: String,
    origin: HwOrigin,
}

impl HwReadFact {
    pub fn new(
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwGuard {
    frames: Vec<HwGuardFrame>,
}

impl HwGuard {
    pub fn new(frames: Vec<HwGuardFrame>) -> Self {
        Self { frames }
    }

    pub fn frames(&self) -> &[HwGuardFrame] {
        &self.frames
    }

    pub fn is_root(&self) -> bool {
        self.frames.is_empty()
    }

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

#[derive(Debug)]
#[non_exhaustive]
pub struct HwCreateFact {
    module: String,
    name: String,
    object_id: ObjectId,
    kind: HwCreateKind,
    origin: HwOrigin,
}

impl HwCreateFact {
    pub fn new(
        module: impl Into<String>,
        name: impl Into<String>,
        object_id: ObjectId,
        kind: HwCreateKind,
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

    pub fn kind(&self) -> HwCreateKind {
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
pub struct HwOrigin {
    source: SourceId,
    span_start: usize,
    span_end: usize,
    expansion_stack: Vec<HwExpansion>,
}

impl HwOrigin {
    pub fn new(
        source: SourceId,
        span_start: usize,
        span_end: usize,
        expansion_stack: Vec<HwExpansion>,
    ) -> Self {
        Self {
            source,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwCreateKind {
    Signal,
    Storage,
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HwModule {
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

    pub fn items(&self) -> &[HwItem] {
        &self.items
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HwParam {
    name: String,
    default: String,
}

impl HwParam {
    pub fn new(name: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default: default.into(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn default(&self) -> &str {
        &self.default
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HwPort {
    direction: HwDirection,
    width: String,
    name: String,
}

impl HwPort {
    pub fn new(direction: HwDirection, width: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            direction,
            width: width.into(),
            name: name.into(),
        }
    }

    pub fn direction(&self) -> HwDirection {
        self.direction
    }

    pub fn width(&self) -> &str {
        &self.width
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwDirection {
    In,
    Out,
}

#[derive(Debug)]
#[non_exhaustive]
pub enum HwItem {
    StaticParam {
        name: String,
        value: HwExpr,
    },
    SignalDecl {
        width: String,
        name: String,
    },
    StorageDecl {
        width: String,
        name: String,
    },
    ContinuousDrive {
        lhs: HwExpr,
        rhs: HwExpr,
    },
    ClockedStorage {
        clock: HwExpr,
        target: HwExpr,
        reset: Option<HwReset>,
        next: HwExpr,
    },
    Instance(HwInstance),
    StaticIf {
        cond: HwExpr,
        label: String,
        then_items: Vec<HwItem>,
        else_items: Vec<HwItem>,
    },
    StaticFor {
        index: String,
        start: HwExpr,
        end: HwExpr,
        label: String,
        items: Vec<HwItem>,
    },
    InitialError {
        message: HwExpr,
    },
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HwReset {
    condition: HwExpr,
    value: HwExpr,
}

impl HwReset {
    pub fn new(condition: HwExpr, value: HwExpr) -> Self {
        Self { condition, value }
    }

    pub fn condition(&self) -> &HwExpr {
        &self.condition
    }

    pub fn value(&self) -> &HwExpr {
        &self.value
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HwInstance {
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
            module: module.into(),
            params,
            name: name.into(),
            connections,
        }
    }

    pub fn module(&self) -> &str {
        &self.module
    }

    pub fn params(&self) -> &[HwParamBind] {
        &self.params
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn connections(&self) -> &[HwConnection] {
        &self.connections
    }
}

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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

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

    pub fn formal(&self) -> &str {
        &self.formal
    }

    pub fn actual(&self) -> &HwExpr {
        &self.actual
    }
}
