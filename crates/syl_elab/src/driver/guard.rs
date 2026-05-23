use crate::eir_guard::EirGuard;

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
