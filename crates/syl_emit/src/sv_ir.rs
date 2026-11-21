pub(super) mod printer;

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

    #[cfg(test)]
    pub(super) fn emit_text(&self) -> String {
        printer::text(&self.modules, true)
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvDirection {
    Input,
    InOut,
    Output,
}
impl From<SvDirection> for &'static str {
    fn from(value: SvDirection) -> Self {
        match value {
            SvDirection::Input => "input",
            SvDirection::InOut => "inout",
            SvDirection::Output => "output",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvItem {
    Source(String),
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
    MemoryDecl {
        name: String,
        elem_width: String,
        depth: String,
    },
    MemoryWrite {
        clock: SvExpr,
        memory: String,
        enable: SvExpr,
        addr: SvExpr,
        data: SvExpr,
    },
    MemorySyncRead {
        clock: SvExpr,
        memory: String,
        enable: SvExpr,
        addr: SvExpr,
        target: SvExpr,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvSelectMode {
    Priority,
    Unique,
}
impl From<SvSelectMode> for &'static str {
    fn from(value: SvSelectMode) -> Self {
        match value {
            SvSelectMode::Priority => "priority",
            SvSelectMode::Unique => "unique",
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvUnaryOp {
    Neg,
    Not,
}
impl From<SvUnaryOp> for &'static str {
    fn from(value: SvUnaryOp) -> Self {
        match value {
            SvUnaryOp::Neg => "-",
            SvUnaryOp::Not => "!",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvBinaryOp {
    OrOr,
    AndAnd,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    BitAnd,
    BitOr,
    BitXor,
}
impl From<SvBinaryOp> for &'static str {
    fn from(value: SvBinaryOp) -> Self {
        match value {
            SvBinaryOp::OrOr => "||",
            SvBinaryOp::AndAnd => "&&",
            SvBinaryOp::Eq => "==",
            SvBinaryOp::NotEq => "!=",
            SvBinaryOp::Lt => "<",
            SvBinaryOp::LtEq => "<=",
            SvBinaryOp::Gt => ">",
            SvBinaryOp::GtEq => ">=",
            SvBinaryOp::Add => "+",
            SvBinaryOp::Sub => "-",
            SvBinaryOp::Mul => "*",
            SvBinaryOp::Div => "/",
            SvBinaryOp::Rem => "%",
            SvBinaryOp::Shl => "<<",
            SvBinaryOp::BitAnd => "&",
            SvBinaryOp::BitOr => "|",
            SvBinaryOp::BitXor => "^",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub(super) struct SvReset {
    pub(super) mode: SvResetMode,
    pub(super) polarity: SvResetPolarity,
    pub(super) condition: SvExpr,
    pub(super) value: SvExpr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvResetMode {
    Sync,
    Async,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(super) enum SvResetPolarity {
    ActiveHigh,
    ActiveLow,
}

impl SvReset {
    pub(super) fn new(condition: SvExpr, value: SvExpr) -> Self {
        Self {
            condition,
            value,
            mode: SvResetMode::Sync,
            polarity: SvResetPolarity::ActiveHigh,
        }
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
