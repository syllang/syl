#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwExpr {
    Ident(String),
    Int(u64),
    Bool(bool),
    Str(String),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwUnaryOp {
    Neg,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwBinaryOp {
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwSelectMode {
    Priority,
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
