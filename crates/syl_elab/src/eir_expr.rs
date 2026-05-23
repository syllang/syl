#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct EirBound {
    source: String,
    expr: Box<EirExpr>,
}

impl EirBound {
    pub(crate) fn new(source: impl Into<String>, expr: EirExpr) -> Self {
        Self {
            source: source.into(),
            expr: Box::new(expr),
        }
    }

    pub(crate) fn from_source(source: impl Into<String>) -> Self {
        let source = source.into();
        if let Ok(value) = source.parse::<u64>() {
            return Self::new(source, EirExpr::Int(value));
        }
        Self::new(source.clone(), EirExpr::ident(source))
    }

    pub(crate) fn from_expr(expr: EirExpr) -> Self {
        Self::new(expr.fact_key(), expr)
    }

    pub(crate) fn source(&self) -> &str {
        &self.source
    }

    pub(crate) fn expr(&self) -> &EirExpr {
        &self.expr
    }

    pub(crate) fn is_one(&self) -> bool {
        matches!(self.expr(), EirExpr::Int(1)) || self.source == "1"
    }
}

impl From<String> for EirBound {
    fn from(value: String) -> Self {
        Self::from_source(value)
    }
}

impl From<&str> for EirBound {
    fn from(value: &str) -> Self {
        Self::from_source(value)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirExpr {
    Ident(String),
    Int(u64),
    Bool(bool),
    Str(String),
    Zero,
    Unary {
        op: EirUnaryOp,
        expr: Box<EirExpr>,
    },
    Binary {
        op: EirBinaryOp,
        left: Box<EirExpr>,
        right: Box<EirExpr>,
    },
    Mux {
        cond: Box<EirExpr>,
        then_value: Box<EirExpr>,
        else_value: Box<EirExpr>,
    },
    Select {
        mode: EirSelectMode,
        arms: Vec<EirSelectArm>,
        default: Box<EirExpr>,
    },
    Concat(Vec<EirExpr>),
    Slice {
        value: Box<EirExpr>,
        high: EirBound,
        low: EirBound,
    },
    IndexedPartSelect {
        value: Box<EirExpr>,
        index: Box<EirExpr>,
        width: EirBound,
    },
    Index {
        value: Box<EirExpr>,
        index: Box<EirExpr>,
    },
    Call {
        name: String,
        args: Vec<EirExpr>,
    },
    Unsupported {
        message: String,
    },
}

impl EirExpr {
    pub(crate) fn ident(name: impl Into<String>) -> Self {
        Self::Ident(name.into())
    }

    pub(crate) fn zero() -> Self {
        Self::Zero
    }

    pub(crate) fn unary(op: EirUnaryOp, expr: EirExpr) -> Self {
        Self::Unary {
            op,
            expr: Box::new(expr),
        }
    }

    pub(crate) fn binary(op: EirBinaryOp, left: EirExpr, right: EirExpr) -> Self {
        Self::Binary {
            op,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    pub(crate) fn mux(cond: EirExpr, then_value: EirExpr, else_value: EirExpr) -> Self {
        Self::Mux {
            cond: Box::new(cond),
            then_value: Box::new(then_value),
            else_value: Box::new(else_value),
        }
    }

    pub(crate) fn select(mode: EirSelectMode, arms: Vec<EirSelectArm>, default: EirExpr) -> Self {
        Self::Select {
            mode,
            arms,
            default: Box::new(default),
        }
    }

    pub(crate) fn index(value: EirExpr, index: EirExpr) -> Self {
        Self::Index {
            value: Box::new(value),
            index: Box::new(index),
        }
    }

    pub(crate) fn indexed_part_select(
        value: EirExpr,
        index: EirExpr,
        width: impl Into<EirBound>,
    ) -> Self {
        Self::IndexedPartSelect {
            value: Box::new(value),
            index: Box::new(index),
            width: width.into(),
        }
    }

    pub(crate) fn slice(
        value: EirExpr,
        high: impl Into<EirBound>,
        low: impl Into<EirBound>,
    ) -> Self {
        Self::Slice {
            value: Box::new(value),
            high: high.into(),
            low: low.into(),
        }
    }

    pub(crate) fn call(name: impl Into<String>, args: Vec<EirExpr>) -> Self {
        Self::Call {
            name: name.into(),
            args,
        }
    }

    pub(crate) fn unsupported(message: impl Into<String>) -> Self {
        Self::Unsupported {
            message: message.into(),
        }
    }

    pub(crate) fn fact_key(&self) -> String {
        match self {
            Self::Ident(name) => name.clone(),
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Str(value) => format!("str({value})"),
            Self::Zero => "zero".to_string(),
            Self::Unary { op, expr } => format!("{}({})", op.fact_name(), expr.fact_key()),
            Self::Binary { op, left, right } => {
                format!(
                    "{}({},{})",
                    op.fact_name(),
                    left.fact_key(),
                    right.fact_key()
                )
            }
            Self::Mux {
                cond,
                then_value,
                else_value,
            } => format!(
                "mux({},{},{})",
                cond.fact_key(),
                then_value.fact_key(),
                else_value.fact_key()
            ),
            Self::Select {
                mode,
                arms,
                default,
            } => {
                let mode = match mode {
                    EirSelectMode::Priority => "priority",
                    EirSelectMode::Unique => "unique",
                };
                let arms = arms
                    .iter()
                    .map(EirSelectArm::fact_key)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("select_{mode}({arms};{})", default.fact_key())
            }
            Self::Concat(parts) => {
                let parts = parts
                    .iter()
                    .map(Self::fact_key)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("concat({parts})")
            }
            Self::Slice { value, high, low } => {
                format!(
                    "slice({},{},{})",
                    value.fact_key(),
                    high.expr().fact_key(),
                    low.expr().fact_key()
                )
            }
            Self::IndexedPartSelect {
                value,
                index,
                width,
            } => format!(
                "part({},{},{})",
                value.fact_key(),
                index.fact_key(),
                width.expr().fact_key()
            ),
            Self::Index { value, index } => {
                format!("idx({},{})", value.fact_key(), index.fact_key())
            }
            Self::Call { name, args } => {
                let args = args
                    .iter()
                    .map(Self::fact_key)
                    .collect::<Vec<_>>()
                    .join(",");
                format!("call({name},{args})")
            }
            Self::Unsupported { message } => format!("unsupported({message})"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirSelectMode {
    Priority,
    Unique,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct EirSelectArm {
    guard: EirExpr,
    value: EirExpr,
}

impl EirSelectArm {
    pub(crate) fn new(guard: EirExpr, value: EirExpr) -> Self {
        Self { guard, value }
    }

    pub(crate) fn guard(&self) -> &EirExpr {
        &self.guard
    }

    pub(crate) fn value(&self) -> &EirExpr {
        &self.value
    }

    fn fact_key(&self) -> String {
        format!("{}=>{}", self.guard.fact_key(), self.value.fact_key())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) enum EirUnaryOp {
    Neg,
    Not,
}

impl EirUnaryOp {
    fn fact_name(self) -> &'static str {
        match self {
            Self::Neg => "neg",
            Self::Not => "not",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) enum EirBinaryOp {
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

impl EirBinaryOp {
    fn fact_name(self) -> &'static str {
        match self {
            Self::OrOr => "logic_or",
            Self::AndAnd => "logic_and",
            Self::Eq => "eq",
            Self::NotEq => "not_eq",
            Self::Lt => "lt",
            Self::LtEq => "lt_eq",
            Self::Gt => "gt",
            Self::GtEq => "gt_eq",
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::Rem => "rem",
            Self::Shl => "shl",
            Self::BitAnd => "bit_and",
            Self::BitOr => "bit_or",
            Self::BitXor => "bit_xor",
        }
    }
}
