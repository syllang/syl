use strum_macros::IntoStaticStr;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwExpr {
    Ident(String),
    Int(u64),
    Bool(bool),
    Str(String),
    HighZ,
    Zero,
    Unary {
        op: HwUnaryOp,
        expr: Box<HwExpr>,
    },
    Binary {
        op: HwBinaryOp,
        left: Box<HwExpr>,
        right: Box<HwExpr>,
    },
    Mux {
        cond: Box<HwExpr>,
        then_value: Box<HwExpr>,
        else_value: Box<HwExpr>,
    },
    Select {
        mode: HwSelectMode,
        arms: Vec<HwSelectArm>,
        default: Box<HwExpr>,
    },
    Concat(Vec<HwExpr>),
    Slice {
        value: Box<HwExpr>,
        high: String,
        low: String,
    },
    IndexedPartSelect {
        value: Box<HwExpr>,
        index: Box<HwExpr>,
        width: String,
    },
    Index {
        value: Box<HwExpr>,
        index: Box<HwExpr>,
    },
    Call {
        name: String,
        args: Vec<HwExpr>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HwUnaryOp {
    #[strum(serialize = "-")]
    Neg,
    #[strum(serialize = "!")]
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HwBinaryOp {
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
    #[strum(serialize = "and")]
    BitAnd,
    #[strum(serialize = "or")]
    BitOr,
    #[strum(serialize = "xor")]
    BitXor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HwSelectMode {
    #[strum(serialize = "priority")]
    Priority,
    #[strum(serialize = "unique")]
    Unique,
}

impl HwSelectMode {
    pub fn is_unique(&self) -> bool {
        matches!(self, Self::Unique)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwSelectArm {
    guard: HwExpr,
    value: HwExpr,
}

impl HwSelectArm {
    pub fn new(guard: HwExpr, value: HwExpr) -> Self {
        Self { guard, value }
    }

    pub fn guard(&self) -> &HwExpr {
        &self.guard
    }

    pub fn value(&self) -> &HwExpr {
        &self.value
    }
}
