#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct DatabaseRevision {
    value: u64,
}

impl DatabaseRevision {
    pub fn initial() -> Self {
        Self { value: 0 }
    }

    pub fn get(self) -> u64 {
        self.value
    }

    pub(crate) fn next(self) -> Self {
        Self {
            value: self.value.saturating_add(1),
        }
    }
}
