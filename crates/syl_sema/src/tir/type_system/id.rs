/// Type identity allocated within one type arena.
///
/// Equality, ordering, and hashing are numeric arena identity. The value is not
/// stable across independent sessions unless explicitly remapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct TypeId(pub usize);

impl TypeId {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }
}
