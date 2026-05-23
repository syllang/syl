use super::{HirCallableItem, HirExternModuleItem, HirSignatureParam, HirSignatureResultBinding};

#[derive(Clone)]
#[non_exhaustive]
pub enum HirCallable {
    Cell(HirCallableItem),
    Module(HirCallableItem),
    Extern(HirExternModuleItem),
}

impl HirCallable {
    pub fn params(&self) -> &[HirSignatureParam] {
        match self {
            Self::Cell(item) | Self::Module(item) => &item.params,
            Self::Extern(item) => &item.params,
        }
    }

    pub fn result(&self) -> Option<&HirSignatureResultBinding> {
        match self {
            Self::Cell(item) | Self::Module(item) => item.result.as_ref(),
            Self::Extern(item) => item.result.as_ref(),
        }
    }

    pub fn callable_item(&self) -> Option<&HirCallableItem> {
        match self {
            Self::Cell(item) | Self::Module(item) => Some(item),
            Self::Extern(_) => None,
        }
    }

    pub(crate) fn summary_count(&self) -> usize {
        match self {
            Self::Cell(item) => 1 + item.summary_count(),
            Self::Module(item) => 2 + item.summary_count(),
            Self::Extern(item) => 3 + item.summary_count(),
        }
    }
}
