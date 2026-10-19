use strum_macros::IntoStaticStr;

mod printer;

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvDesign {
    pub(super) modules: Vec<SvModule>,
}

impl SvDesign {
    pub(super) fn new(modules: Vec<SvModule>) -> Self {
        Self { modules }
    }

    pub(super) fn debug_dump(&self) -> String {
        let modules = self
            .modules
            .iter()
            .map(|module| module.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        format!("sv_ast modules={} [{}]", self.modules.len(), modules)
    }

    pub(super) fn emit_text(&self) -> String {
        printer::text(self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvModule {
    pub(super) doc: Option<String>,
    pub(super) name: String,
    pub(super) params: Vec<SvParam>,
    pub(super) ports: Vec<SvPort>,
    pub(super) items: Vec<SvItem>,
}

impl SvModule {
    pub(super) fn new(
        name: impl Into<String>,
        params: Vec<SvParam>,
        ports: Vec<SvPort>,
        items: Vec<SvItem>,
    ) -> Self {
        Self {
            doc: None,
            name: name.into(),
            params,
            ports,
            items,
        }
    }

    pub(super) fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvParam {
    pub(super) doc: Option<String>,
    pub(super) name: String,
    pub(super) default: String,
}

impl SvParam {
    pub(super) fn new(name: impl Into<String>, default: impl Into<String>) -> Self {
        Self {
            doc: None,
            name: name.into(),
            default: default.into(),
        }
    }

    pub(super) fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvPort {
    pub(super) doc: Option<String>,
    pub(super) direction: SvDirection,
    pub(super) width: String,
    pub(super) name: String,
}

impl SvPort {
    pub(super) fn new(
        direction: SvDirection,
        width: impl Into<String>,
        name: impl Into<String>,
    ) -> Self {
        Self {
            doc: None,
            direction,
            width: width.into(),
            name: name.into(),
        }
    }

    pub(super) fn with_doc(mut self, doc: Option<String>) -> Self {
        self.doc = doc;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub(super) enum SvDirection {
    #[strum(serialize = "input")]
    Input,
    #[strum(serialize = "inout")]
    InOut,
    #[strum(serialize = "output")]
    Output,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvItem {
    LocalParam {
        name: String,
        value: SvExpr,
    },
    Wire {
        width: String,
        name: String,
    },
    Reg {
        width: String,
        name: String,
    },
    Assign {
        lhs: SvExpr,
        rhs: SvExpr,
    },
    AlwaysReg {
        clock: SvExpr,
        target: SvExpr,
        reset: Option<SvReset>,
        next: SvExpr,
    },
    AlwaysAssert {
        clock: SvExpr,
        trigger: SvExpr,
        message: SvExpr,
    },
    Instance(SvInstance),
    GenerateIf {
        cond: SvExpr,
        label: String,
        then_items: Vec<SvItem>,
        else_items: Vec<SvItem>,
    },
    GenerateFor {
        genvar: String,
        start: SvExpr,
        end: SvExpr,
        label: String,
        items: Vec<SvItem>,
    },
    InitialError {
        message: SvExpr,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvExpr {
    Ident(String),
    Int(u64),
    Bool(bool),
    Str(String),
    HighZ,
    Zero,
    Unary {
        op: SvUnaryOp,
        expr: Box<SvExpr>,
    },
    Binary {
        op: SvBinaryOp,
        left: Box<SvExpr>,
        right: Box<SvExpr>,
    },
    Mux {
        cond: Box<SvExpr>,
        then_value: Box<SvExpr>,
        else_value: Box<SvExpr>,
    },
    Select {
        mode: SvSelectMode,
        arms: Vec<SvSelectArm>,
        default: Box<SvExpr>,
    },
    Concat(Vec<SvExpr>),
    Slice {
        value: Box<SvExpr>,
        high: String,
        low: String,
    },
    IndexedPartSelect {
        value: Box<SvExpr>,
        index: Box<SvExpr>,
        width: String,
    },
    Index {
        value: Box<SvExpr>,
        index: Box<SvExpr>,
    },
    Call {
        name: String,
        args: Vec<SvExpr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub(super) enum SvSelectMode {
    #[strum(serialize = "priority")]
    Priority,
    #[strum(serialize = "unique")]
    Unique,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvSelectArm {
    pub(super) guard: SvExpr,
    pub(super) value: SvExpr,
}

impl SvSelectArm {
    pub(super) fn new(guard: SvExpr, value: SvExpr) -> Self {
        Self { guard, value }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub(super) enum SvUnaryOp {
    #[strum(serialize = "-")]
    Neg,
    #[strum(serialize = "!")]
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub(super) enum SvBinaryOp {
    #[strum(serialize = "||")]
    OrOr,
    #[strum(serialize = "&&")]
    AndAnd,
    #[strum(serialize = "==")]
    Eq,
    #[strum(serialize = "!=")]
    NotEq,
    #[strum(serialize = "<")]
    Lt,
    #[strum(serialize = "<=")]
    LtEq,
    #[strum(serialize = ">")]
    Gt,
    #[strum(serialize = ">=")]
    GtEq,
    #[strum(serialize = "+")]
    Add,
    #[strum(serialize = "-")]
    Sub,
    #[strum(serialize = "*")]
    Mul,
    #[strum(serialize = "/")]
    Div,
    #[strum(serialize = "%")]
    Rem,
    #[strum(serialize = "<<")]
    Shl,
    #[strum(serialize = "&")]
    BitAnd,
    #[strum(serialize = "|")]
    BitOr,
    #[strum(serialize = "^")]
    BitXor,
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvReset {
    pub(super) condition: SvExpr,
    pub(super) value: SvExpr,
}

impl SvReset {
    pub(super) fn new(condition: SvExpr, value: SvExpr) -> Self {
        Self { condition, value }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvInstance {
    pub(super) module: String,
    pub(super) params: Vec<SvParamBind>,
    pub(super) name: String,
    pub(super) connections: Vec<SvConnection>,
}

impl SvInstance {
    pub(super) fn new(
        module: impl Into<String>,
        params: Vec<SvParamBind>,
        name: impl Into<String>,
        connections: Vec<SvConnection>,
    ) -> Self {
        Self {
            module: module.into(),
            params,
            name: name.into(),
            connections,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvParamBind {
    pub(super) name: String,
    pub(super) value: String,
}

impl SvParamBind {
    pub(super) fn new(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            value: value.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvConnection {
    pub(super) formal: String,
    pub(super) actual: SvExpr,
}

impl SvConnection {
    pub(super) fn new(formal: impl Into<String>, actual: SvExpr) -> Self {
        Self {
            formal: formal.into(),
            actual,
        }
    }
}
