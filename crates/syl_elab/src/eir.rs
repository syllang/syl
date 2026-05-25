use crate::{
    CellBoundarySummary,
    eir_cell::EirCellExpansion,
    eir_expr::{EirBound, EirExpr},
    eir_guard::EirGuard,
    eir_origin::EirOrigin,
    eir_place::EirPlace,
};
use std::sync::Arc;

mod assemble;
mod facts;
mod validate;

pub(crate) use assemble::EirDesignComposer;
pub(crate) use facts::EirFactCollector;
pub(crate) use validate::EirValidator;

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
    fn from_parts(raw: Arc<EirRawDesign>, facts: Arc<EirDesignFacts>) -> Self {
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
    pub(crate) fn new(objects: Vec<EirObject>, drives: Vec<EirDrive>, reads: Vec<EirRead>) -> Self {
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

#[non_exhaustive]
pub(crate) struct EirObject {
    module: String,
    name: String,
    width: EirBound,
    kind: EirObjectKind,
    activity: EirSignalActivity,
    origin: EirOrigin,
}

pub(crate) struct EirObjectInput {
    pub(crate) module: String,
    pub(crate) name: String,
    pub(crate) width: EirBound,
    pub(crate) kind: EirObjectKind,
    pub(crate) activity: EirSignalActivity,
    pub(crate) origin: EirOrigin,
}

impl EirObject {
    fn new(input: EirObjectInput) -> Self {
        Self {
            module: input.module,
            name: input.name,
            width: input.width,
            kind: input.kind,
            activity: input.activity,
            origin: input.origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn width_bound(&self) -> &EirBound {
        &self.width
    }

    pub(crate) fn kind(&self) -> EirObjectKind {
        self.kind
    }

    pub(crate) fn activity(&self) -> EirSignalActivity {
        self.activity
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(crate) enum EirObjectKind {
    Signal,
    Storage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirSignalActivity {
    Required,
    Optional,
}

#[non_exhaustive]
pub(crate) struct EirDrive {
    module: String,
    target: EirPlace,
    value: Option<EirExpr>,
    kind: EirDriveKind,
    guard: EirGuard,
    origin: EirOrigin,
}

impl EirDrive {
    #[allow(clippy::too_many_arguments)]
    fn new(
        module: impl Into<String>,
        target: EirPlace,
        kind: EirDriveKind,
        value: Option<EirExpr>,
        guard: EirGuard,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            target,
            value,
            kind,
            guard,
            origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn target_place(&self) -> &EirPlace {
        &self.target
    }

    pub(crate) fn value(&self) -> Option<&EirExpr> {
        self.value.as_ref()
    }

    pub(crate) fn kind(&self) -> EirDriveKind {
        self.kind
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
pub(crate) enum EirDriveKind {
    Continuous,
    Next,
}

#[non_exhaustive]
pub(crate) struct EirRead {
    module: String,
    source: EirPlace,
    guard: EirGuard,
    origin: EirOrigin,
}

impl EirRead {
    fn new(
        module: impl Into<String>,
        source: EirPlace,
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

    pub(crate) fn source_place(&self) -> &EirPlace {
        &self.source
    }

    pub(crate) fn guard(&self) -> &EirGuard {
        &self.guard
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirModule {
    name: String,
    kind: EirModuleKind,
    params: Vec<EirParam>,
    ports: Vec<EirPort>,
    items: Vec<EirItem>,
}

impl EirModule {
    pub(crate) fn new(
        name: impl Into<String>,
        params: Vec<EirParam>,
        ports: Vec<EirPort>,
        items: Vec<EirItem>,
    ) -> Self {
        Self {
            name: name.into(),
            kind: EirModuleKind::Defined,
            params,
            ports,
            items,
        }
    }

    pub(crate) fn new_extern(
        name: impl Into<String>,
        params: Vec<EirParam>,
        ports: Vec<EirPort>,
    ) -> Self {
        Self {
            name: name.into(),
            kind: EirModuleKind::Extern,
            params,
            ports,
            items: Vec::new(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn is_extern(&self) -> bool {
        self.kind == EirModuleKind::Extern
    }

    pub(crate) fn params(&self) -> &[EirParam] {
        &self.params
    }

    pub(crate) fn ports(&self) -> &[EirPort] {
        &self.ports
    }

    pub(crate) fn items(&self) -> &[EirItem] {
        &self.items
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EirModuleKind {
    Defined,
    Extern,
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirParam {
    name: String,
    default: String,
}

impl EirParam {
    pub(crate) fn new(name: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            default: default.into(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn default(&self) -> &str {
        &self.default
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirPort {
    direction: EirDirection,
    width: EirBound,
    name: String,
    origin: EirOrigin,
}

impl EirPort {
    pub(crate) fn new(
        direction: EirDirection,
        width: impl Into<EirBound>,
        name: impl Into<String>,
        origin: EirOrigin,
    ) -> Self {
        Self {
            direction,
            width: width.into(),
            name: name.into(),
            origin,
        }
    }

    pub(crate) fn direction(&self) -> EirDirection {
        self.direction
    }

    pub(crate) fn width(&self) -> &str {
        self.width.source()
    }

    pub(crate) fn width_bound(&self) -> &EirBound {
        &self.width
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirDirection {
    In,
    InOut,
    Out,
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) enum EirItem {
    StaticParam {
        name: String,
        value: EirExpr,
        origin: EirOrigin,
    },
    Signal {
        width: EirBound,
        name: String,
        activity: EirSignalActivity,
        origin: EirOrigin,
    },
    Storage {
        width: EirBound,
        name: String,
        origin: EirOrigin,
    },
    Drive {
        lhs: EirPlace,
        rhs: EirExpr,
        reads: Vec<EirExpr>,
        origin: EirOrigin,
    },
    ClockedStorage {
        clock: EirExpr,
        target: EirPlace,
        reset: Option<Box<EirReset>>,
        next: EirExpr,
        reads: Vec<EirExpr>,
        origin: EirOrigin,
    },
    CellExpansion(EirCellExpansion),
    /// Model-only opaque/precompiled boundary until source-level summary declarations exist.
    #[allow(dead_code)]
    CellBoundary(CellBoundarySummary),
    Instance(EirInstance),
    /// Symbolic elaboration guard kept only when the condition still depends on
    /// generic/localparam values after Const MIR evaluation.
    SymbolicStaticIf {
        cond: EirExpr,
        label: String,
        then_items: Vec<EirItem>,
        else_items: Vec<EirItem>,
        origin: EirOrigin,
    },
    /// Symbolic elaboration loop kept only when the range is finite but not
    /// numerically known until backend parameterization.
    SymbolicStaticFor {
        index: String,
        start: EirExpr,
        end: EirExpr,
        label: String,
        items: Vec<EirItem>,
        origin: EirOrigin,
    },
    InitialError {
        message: EirExpr,
        origin: EirOrigin,
    },
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirReset {
    condition: EirExpr,
    value: EirExpr,
}

impl EirReset {
    pub(crate) fn new(condition: EirExpr, value: EirExpr) -> Self {
        Self { condition, value }
    }

    pub(crate) fn condition(&self) -> &EirExpr {
        &self.condition
    }

    pub(crate) fn value(&self) -> &EirExpr {
        &self.value
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirInstance {
    module: String,
    params: Vec<EirParamBind>,
    name: String,
    connections: Vec<EirConnection>,
    origin: EirOrigin,
}

impl EirInstance {
    pub(crate) fn new(
        module: impl Into<String>,
        params: Vec<EirParamBind>,
        name: impl Into<String>,
        connections: Vec<EirConnection>,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            params,
            name: name.into(),
            connections,
            origin,
        }
    }

    pub(crate) fn module(&self) -> &str {
        &self.module
    }

    pub(crate) fn params(&self) -> &[EirParamBind] {
        &self.params
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn connections(&self) -> &[EirConnection] {
        &self.connections
    }

    pub(crate) fn origin(&self) -> &EirOrigin {
        &self.origin
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirParamBind {
    name: String,
    value: String,
}

impl EirParamBind {
    pub(crate) fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirConnection {
    formal: String,
    actual: EirExpr,
}

impl EirConnection {
    pub(crate) fn new(formal: impl Into<String>, actual: EirExpr) -> Self {
        Self {
            formal: formal.into(),
            actual,
        }
    }

    pub(crate) fn formal(&self) -> &str {
        &self.formal
    }

    pub(crate) fn actual(&self) -> &EirExpr {
        &self.actual
    }
}
