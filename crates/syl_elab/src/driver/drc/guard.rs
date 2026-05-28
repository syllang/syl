use crate::eir::EirGuard;

/// Checks whether two guards are mutually exclusive (cannot be active simultaneously).
///
/// **Stack semantics:** Guards form stacks. Two guards are mutually exclusive iff
/// they share a common prefix of frames, and the first differing frame is an
/// `IfThen`/`IfElse` opposite pair with matching labels.
///
/// **Non-exclusive cases:**
/// - `[IfThen("a")]` vs `[IfThen("b")]` — different labels, not exclusive.
/// - `[IfThen("a")]` vs `[IfThen("a"), IfThen("b")]` — one is a prefix of the
///   other (nested scope), not exclusive.
/// - Root guard vs anything — always overlaps.
/// - Equal guards — always overlaps.
///
/// **Why `zip` stops early:** The `zip` only iterates as far as the shorter
/// frame list. If one guard is a prefix of the other, no difference is detected
/// after the loop, and the function returns `false` — correctly, since a
/// deeper scope is *inside* an outer scope, not exclusive with it.
#[non_exhaustive]
pub(super) struct DriverGuardSet<'a> {
    first: &'a EirGuard,
    second: &'a EirGuard,
}

impl<'a> DriverGuardSet<'a> {
    pub(super) fn new(first: &'a EirGuard, second: &'a EirGuard) -> Self {
        Self { first, second }
    }

    pub(super) fn is_mutually_exclusive(&self) -> bool {
        if self.first.is_root() || self.second.is_root() || self.first == self.second {
            return false;
        }
        for (left, right) in self.first.frames().iter().zip(self.second.frames()) {
            if left == right {
                continue;
            }
            return left.is_opposite_if_branch(right);
        }
        false
    }
}
