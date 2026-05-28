/// Package identity allocated by one compiler session.
///
/// Equality, ordering, and hashing are numeric arena identity. The value is not
/// stable across independent sessions unless explicitly remapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct PackageId(pub usize);

impl PackageId {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

/// Definition identity allocated by one HIR arena.
///
/// Equality, ordering, and hashing are numeric arena identity. The value is not
/// stable across independent sessions unless explicitly remapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct DefId(pub usize);

impl DefId {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

/// Local binding identity allocated within one HIR arena.
///
/// Equality, ordering, and hashing are numeric arena identity. The value is not
/// stable across independent sessions unless explicitly remapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct LocalId(pub usize);

impl LocalId {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }
}

/// Expression identity allocated within one HIR arena.
///
/// Equality, ordering, and hashing are numeric arena identity. The value is not
/// stable across independent sessions unless explicitly remapped.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ExprId(pub usize);

impl ExprId {
    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }
}
