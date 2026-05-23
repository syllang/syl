use crate::eir_expr::{EirBound, EirExpr};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirPlace {
    Ident(String),
    Slice {
        base: Box<EirPlace>,
        high: EirBound,
        low: EirBound,
    },
    IndexedPartSelect {
        base: Box<EirPlace>,
        index: EirExpr,
        width: EirBound,
    },
    Index {
        base: Box<EirPlace>,
        index: EirExpr,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct EirPlaceError;

impl EirPlace {
    pub(crate) fn to_expr(&self) -> EirExpr {
        match self {
            Self::Ident(name) => EirExpr::Ident(name.clone()),
            Self::Slice { base, high, low } => EirExpr::Slice {
                value: Box::new(base.to_expr()),
                high: high.clone(),
                low: low.clone(),
            },
            Self::IndexedPartSelect { base, index, width } => EirExpr::IndexedPartSelect {
                value: Box::new(base.to_expr()),
                index: Box::new(index.clone()),
                width: width.clone(),
            },
            Self::Index { base, index } => EirExpr::Index {
                value: Box::new(base.to_expr()),
                index: Box::new(index.clone()),
            },
        }
    }
}

impl TryFrom<&EirExpr> for EirPlace {
    type Error = EirPlaceError;

    fn try_from(expr: &EirExpr) -> Result<Self, Self::Error> {
        match expr {
            EirExpr::Ident(name) => Ok(Self::Ident(name.clone())),
            EirExpr::Slice { value, high, low } => Ok(Self::Slice {
                base: Box::new(Self::try_from(value.as_ref())?),
                high: high.clone(),
                low: low.clone(),
            }),
            EirExpr::IndexedPartSelect {
                value,
                index,
                width,
            } => Ok(Self::IndexedPartSelect {
                base: Box::new(Self::try_from(value.as_ref())?),
                index: index.as_ref().clone(),
                width: width.clone(),
            }),
            EirExpr::Index { value, index } => Ok(Self::Index {
                base: Box::new(Self::try_from(value.as_ref())?),
                index: index.as_ref().clone(),
            }),
            _ => Err(EirPlaceError),
        }
    }
}
