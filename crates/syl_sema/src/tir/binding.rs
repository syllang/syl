use syl_hir::{DefId, LocalId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BindingRef {
    Def(DefId),
    Local(LocalId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BindingKind {
    Const,
    Generic,
    Port,
    Local,
}
