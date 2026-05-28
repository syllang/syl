/// Elaborated hardware object identity allocated within one EIR/HWIR arena.
///
/// Equality, ordering, and hashing are numeric arena identity. The value is not
/// stable across independent sessions unless explicitly remapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ObjectId(pub usize);

impl ObjectId {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }
}
