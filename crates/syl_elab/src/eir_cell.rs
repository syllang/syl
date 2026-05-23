use crate::eir::EirItem;

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct EirCellExpansion {
    callable: String,
    instance: String,
    items: Vec<EirItem>,
}

impl EirCellExpansion {
    pub(crate) fn new(
        callable: impl Into<String>,
        instance: impl Into<String>,
        items: Vec<EirItem>,
    ) -> Self {
        Self {
            callable: callable.into(),
            instance: instance.into(),
            items,
        }
    }

    pub(crate) fn callable(&self) -> &str {
        &self.callable
    }

    pub(crate) fn instance(&self) -> &str {
        &self.instance
    }

    pub(crate) fn items(&self) -> &[EirItem] {
        &self.items
    }
}
