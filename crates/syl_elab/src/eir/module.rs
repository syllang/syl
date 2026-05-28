use super::{EirBound, EirExpr, EirOrigin, EirPlace, EirReset, EirSignalActivity};
#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirModule {
    doc: Option<String>,
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
            doc: None,
            name: name.into(),
            kind: EirModuleKind::Defined,
            params,
            ports,
            items,
        }
    }

    pub(crate) fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }

    pub(crate) fn new_extern(
        name: impl Into<String>,
        params: Vec<EirParam>,
        ports: Vec<EirPort>,
    ) -> Self {
        Self {
            doc: None,
            name: name.into(),
            kind: EirModuleKind::Extern,
            params,
            ports,
            items: Vec::new(),
        }
    }

    pub(crate) fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
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
    doc: Option<String>,
    name: String,
    default: String,
}

impl EirParam {
    pub(crate) fn new(name: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            doc: None,
            name: name.into(),
            default: default.into(),
        }
    }

    pub(crate) fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }

    pub(crate) fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
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
    doc: Option<String>,
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
            doc: None,
            direction,
            width: width.into(),
            name: name.into(),
            origin,
        }
    }

    pub(crate) fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }

    pub(crate) fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
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
    Instance(EirInstance),
    SymbolicStaticIf {
        cond: EirExpr,
        label: String,
        then_items: Vec<EirItem>,
        else_items: Vec<EirItem>,
        origin: EirOrigin,
    },
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
pub(crate) struct EirCellExpansion {
    callable: String,
    instance: String,
    items: Vec<EirItem>,
}

impl EirCellExpansion {
    pub(crate) fn new(
        callable: impl Into<String>,
        instance: impl Into<String>,
        items: Vec<EirItem>,
    ) -> Self {
        Self {
            callable: callable.into(),
            instance: instance.into(),
            items,
        }
    }

    pub(crate) fn callable(&self) -> &str {
        &self.callable
    }

    pub(crate) fn instance(&self) -> &str {
        &self.instance
    }

    pub(crate) fn items(&self) -> &[EirItem] {
        &self.items
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirInstance {
    module: String,
    params: Vec<EirParamBind>,
    name: String,
    source_name: String,
    connections: Vec<EirConnection>,
    origin: EirOrigin,
}

impl EirInstance {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        module: impl Into<String>,
        params: Vec<EirParamBind>,
        name: impl Into<String>,
        source_name: impl Into<String>,
        connections: Vec<EirConnection>,
        origin: EirOrigin,
    ) -> Self {
        Self {
            module: module.into(),
            params,
            name: name.into(),
            source_name: source_name.into(),
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

    pub(crate) fn source_name(&self) -> &str {
        &self.source_name
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
