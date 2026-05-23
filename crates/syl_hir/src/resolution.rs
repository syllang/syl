use crate::{DefId, LocalId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirResolution {
    Def(DefId),
    Local(LocalId),
}
