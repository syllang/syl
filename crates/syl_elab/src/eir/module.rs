#[path = "disambiguate.rs"]
mod disambiguate;
use super::{EirBound, EirExpr, EirOrigin, EirPlace, EirReset, EirSignalActivity};
#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) struct EirModule {
    cdc_crossings: Vec<syl_sema::SummaryCdcCrossing>,
    canonical_path: Vec<String>,
    origin: Option<EirOrigin>,
    doc: Option<String>,
    name: String,
    kind: EirModuleKind,
    params: Vec<EirParam>,
    ports: Vec<EirPort>,
    items: Vec<EirItem>,
}

impl EirModule {
    pub(crate) fn with_cdc_crossings(
        mut self,
        crossings: Vec<syl_sema::SummaryCdcCrossing>,
    ) -> Self {
        self.cdc_crossings = crossings;
        self
    }
    pub(crate) fn cdc_crossings(&self) -> &[syl_sema::SummaryCdcCrossing] {
        &self.cdc_crossings
    }
    pub(crate) fn new(
        name: impl Into<String>,
        params: Vec<EirParam>,
        ports: Vec<EirPort>,
        items: Vec<EirItem>,
    ) -> Self {
        Self {
            cdc_crossings: Vec::new(),
            canonical_path: Vec::new(),
            origin: None,
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
            cdc_crossings: Vec::new(),
            canonical_path: Vec::new(),
            origin: None,
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
    pub(crate) fn with_canonical_path(mut self, path: Vec<String>) -> Self {
        self.canonical_path = path;
        self
    }
    pub(crate) fn canonical_path(&self) -> &[String] {
        &self.canonical_path
    }
    pub(crate) fn with_origin(mut self, origin: Option<EirOrigin>) -> Self {
        self.origin = origin;
        self
    }
    pub(crate) fn origin(&self) -> Option<&EirOrigin> {
        self.origin.as_ref()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EirModuleKind {
    Defined,
    Extern,
}

#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) struct EirParam {
    pub(crate) is_bool: bool,
    pub(crate) default_expr: Option<(EirExpr, syl_span::Span)>,
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
            is_bool: false,
            default_expr: None,
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

#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) struct EirPort {
    pub(crate) source_name: Option<String>,
    domain: EirPortDomain,
    doc: Option<String>,
    direction: EirDirection,
    width: EirBound,
    name: String,
    origin: EirOrigin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirPortDomain {
    Data,
    Clock {
        domain: Option<String>,
    },
    Reset {
        domain: Option<String>,
        mode: super::EirResetMode,
        polarity: super::EirResetPolarity,
    },
}
impl EirPort {
    pub(crate) fn with_domain(mut self, domain: EirPortDomain) -> Self {
        self.domain = domain;
        self
    }
    pub(crate) fn domain(&self) -> &EirPortDomain {
        &self.domain
    }
    pub(crate) fn new(
        direction: EirDirection,
        width: impl Into<EirBound>,
        name: impl Into<String>,
        origin: EirOrigin,
    ) -> Self {
        Self {
            domain: EirPortDomain::Data,
            source_name: None,
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

#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) enum EirItem {
    StaticParam {
        name: String,
        value: EirExpr,
        origin: EirOrigin,
    },
    Signal {
        domain: EirPortDomain,
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
    ClockedAssert {
        clock: EirExpr,
        trigger: EirExpr,
        reads: Vec<EirExpr>,
        message: Vec<syl_hw::FormatPiece<EirExpr>>,
        origin: EirOrigin,
    },
    ClockedLog {
        clock: EirExpr,
        trigger: EirExpr,
        reads: Vec<EirExpr>,
        message: Vec<syl_hw::FormatPiece<EirExpr>>,
        origin: EirOrigin,
    },
    InitialError {
        message: EirExpr,
        origin: EirOrigin,
    },
    MemoryDecl {
        name: String,
        elem_width: EirBound,
        depth: EirBound,
        origin: EirOrigin,
    },
    MemoryWrite {
        clock: EirExpr,
        memory: String,
        enable: EirExpr,
        addr: EirExpr,
        data: EirExpr,
        reads: Vec<EirExpr>,
        origin: EirOrigin,
    },
    MemorySyncRead {
        clock: EirExpr,
        memory: String,
        enable: EirExpr,
        addr: EirExpr,
        target: EirPlace,
        reads: Vec<EirExpr>,
        origin: EirOrigin,
    },
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) struct EirInstance {
    target_canonical_path: Vec<String>,
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
            target_canonical_path: Vec::new(),
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
    pub(crate) fn with_target_canonical_path(mut self, path: Vec<String>) -> Self {
        self.target_canonical_path = path;
        self
    }
    pub(crate) fn target_canonical_path(&self) -> &[String] {
        &self.target_canonical_path
    }
}

#[derive(Clone, Debug)]
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

#[derive(Clone, Debug)]
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
