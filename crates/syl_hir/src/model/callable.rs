use super::{HirCallableItem, HirExternCellItem, HirSignatureParam, HirSignatureResultBinding};

/// A cell or extern cell definition in the HIR.
///
/// Wraps `HirCallableItem` (with body) or `HirExternCellItem` (without body).
#[derive(Clone)]
#[non_exhaustive]
pub enum HirCallable {
    Cell(HirCallableItem),
    Extern(HirExternCellItem),
}

impl HirCallable {
    /// Returns the parameter list of this callable.
    pub fn params(&self) -> &[HirSignatureParam] {
        match self {
            Self::Cell(item) => &item.params,
            Self::Extern(item) => &item.params,
        }
    }

    /// Returns the result binding of this callable, if any.
    pub fn result(&self) -> Option<&HirSignatureResultBinding> {
        match self {
            Self::Cell(item) => item.result.as_ref(),
            Self::Extern(item) => item.result.as_ref(),
        }
    }

    /// Returns the callable item if this is a cell with a body.
    pub fn callable_item(&self) -> Option<&HirCallableItem> {
        match self {
            Self::Cell(item) => Some(item),
            Self::Extern(_) => None,
        }
    }

    pub(crate) fn summary_count(&self) -> usize {
        match self {
            Self::Cell(item) => 1 + item.summary_count(),
            Self::Extern(item) => 3 + item.summary_count(),
        }
    }
}
