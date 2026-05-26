use super::{HirCallableItem, HirExternCellItem, HirSignatureParam, HirSignatureResultBinding};

#[derive(Clone)]
#[non_exhaustive]
pub enum HirCallable {
    Cell(HirCallableItem),
    Extern(HirExternCellItem),
}

impl HirCallable {
    pub fn params(&self) -> &[HirSignatureParam] {
        match self {
            Self::Cell(item) => &item.params,
            Self::Extern(item) => &item.params,
        }
    }

    pub fn result(&self) -> Option<&HirSignatureResultBinding> {
        match self {
            Self::Cell(item) => item.result.as_ref(),
            Self::Extern(item) => item.result.as_ref(),
        }
    }

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
