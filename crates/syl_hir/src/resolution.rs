use crate::{DefId, LocalId};

/// The result of name resolution: either a global definition or a local binding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirResolution {
    Def(DefId),
    Local(LocalId),
}
