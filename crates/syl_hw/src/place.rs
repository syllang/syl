use crate::ObjectId;

/// A hardware signal location — identifies a signal by name, slice, index, or part-select.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwPlace {
    Ident(String),
    Object {
        id: ObjectId,
        name: String,
    },
    Slice {
        base: Box<HwPlace>,
        high: String,
        low: String,
    },
    IndexedPartSelect {
        base: Box<HwPlace>,
        index: HwPlaceExpr,
        width: String,
    },
    Index {
        base: Box<HwPlace>,
        index: HwPlaceExpr,
    },
    Expr(HwPlaceExpr),
}

impl HwPlace {
    pub fn display(&self) -> String {
        match self {
            Self::Ident(name) | Self::Object { name, .. } => name.clone(),
            Self::Slice { base, high, low } => {
                format!("slice({},{high},{low})", base.display())
            }
            Self::IndexedPartSelect { base, index, width } => {
                format!("part({},{},{width})", base.display(), index.display())
            }
            Self::Index { base, index } => format!("idx({},{})", base.display(), index.display()),
            Self::Expr(expr) => expr.display(),
        }
    }
}

/// An expression used in a place (location) — restricted to compile-time evaluable forms.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwPlaceExpr {
    Ident(String),
    Int(u64),
    Bool(bool),
    Str(String),
    Zero,
    Op {
        name: String,
        args: Vec<HwPlaceExpr>,
    },
}

impl HwPlaceExpr {
    pub fn display(&self) -> String {
        match self {
            Self::Ident(name) => name.clone(),
            Self::Int(value) => value.to_string(),
            Self::Bool(value) => value.to_string(),
            Self::Str(value) => format!("str({value})"),
            Self::Zero => "zero".to_string(),
            Self::Op { name, args } => {
                let args = args.iter().map(Self::display).collect::<Vec<_>>().join(",");
                format!("{name}({args})")
            }
        }
    }
}
