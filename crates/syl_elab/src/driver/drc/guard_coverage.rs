use crate::eir::{EirGuard, EirGuardFrame, EirGuardLabel};
use std::collections::BTreeMap;

/// Checks whether a set of guards covers all possible execution paths.
///
/// **How coverage works:** A set of guards is exhaustive if either:
/// 1. Any single guard in the set is an unconditional path (empty frames).
/// 2. The set contains an **exhaustive if-split**: for some label `L`,
///    both the `IfThen(L)` and `IfElse(L)` branches have their own covering
///    sets of *residual* frames (the frames after removing the split frame).
///
/// **Depth limit:** The exhaustive if-split check (`has_exhaustive_if_split`)
/// only works for **single-level** if-else splits. Nested if-elses with the
/// same label would be conflated because `BranchTails` groups by label alone,
/// not by the full frame prefix leading to that label.
///
/// **Residual framing:** `covers_under` strips a common context prefix from
/// each guard before checking coverage. If a drive guard `[A, B, C]` is
/// checked under context `[A, B]`, the residual is `[C]`. This allows checking
/// whether a set of inner-scope drives covers all paths within that outer scope.
#[non_exhaustive]
pub(super) struct GuardCoverage<'a> {
    guards: Vec<&'a EirGuard>,
}

impl<'a> GuardCoverage<'a> {
    pub(super) fn new() -> Self {
        Self { guards: Vec::new() }
    }

    pub(super) fn add(&mut self, guard: &'a EirGuard) {
        self.guards.push(guard);
    }

    pub(super) fn covers_unconditionally(&self) -> bool {
        let frames = self.guards.iter().map(|guard| guard.frames()).collect();
        GuardFrameCoverage::new(frames).covers()
    }

    pub(super) fn covers_under(&self, context: &EirGuard) -> bool {
        let mut frames = Vec::new();
        for guard in &self.guards {
            if let Some(residual) =
                GuardFrameRelation::new(guard.frames(), context.frames()).residual()
            {
                frames.push(residual);
            }
        }
        GuardFrameCoverage::new(frames).covers()
    }
}

#[non_exhaustive]
struct GuardFrameRelation<'a> {
    drive: &'a [EirGuardFrame],
    context: &'a [EirGuardFrame],
}

impl<'a> GuardFrameRelation<'a> {
    fn new(drive: &'a [EirGuardFrame], context: &'a [EirGuardFrame]) -> Self {
        Self { drive, context }
    }

    fn residual(&self) -> Option<&'a [EirGuardFrame]> {
        if let Some(residual) = self.drive.strip_prefix(self.context) {
            return Some(residual);
        }
        if self.context.starts_with(self.drive) {
            return Some(&[]);
        }
        None
    }
}

#[non_exhaustive]
struct GuardFrameCoverage<'a> {
    frames: Vec<&'a [EirGuardFrame]>,
}

impl<'a> GuardFrameCoverage<'a> {
    fn new(frames: Vec<&'a [EirGuardFrame]>) -> Self {
        Self { frames }
    }

    fn covers(&self) -> bool {
        self.frames
            .iter()
            .any(|frames| self.is_unconditional_path(frames))
            || self.has_exhaustive_if_split()
    }

    fn has_exhaustive_if_split(&self) -> bool {
        let mut splits = BTreeMap::<&EirGuardLabel, BranchTails<'a>>::new();
        for frames in &self.frames {
            let Some((head, tail)) = frames.split_first() else {
                continue;
            };
            match head {
                EirGuardFrame::IfThen { label } => {
                    splits.entry(label).or_default().add_then(tail);
                }
                EirGuardFrame::IfElse { label } => {
                    splits.entry(label).or_default().add_else(tail);
                }
                EirGuardFrame::Loop { .. } => {}
            }
        }
        splits.values().any(BranchTails::covers)
    }

    fn is_unconditional_path(&self, frames: &[EirGuardFrame]) -> bool {
        frames.is_empty()
    }
}

#[derive(Default)]
#[non_exhaustive]
struct BranchTails<'a> {
    then_tails: Vec<&'a [EirGuardFrame]>,
    else_tails: Vec<&'a [EirGuardFrame]>,
}

impl<'a> BranchTails<'a> {
    fn add_then(&mut self, frames: &'a [EirGuardFrame]) {
        self.then_tails.push(frames);
    }

    fn add_else(&mut self, frames: &'a [EirGuardFrame]) {
        self.else_tails.push(frames);
    }

    fn covers(&self) -> bool {
        !self.then_tails.is_empty()
            && !self.else_tails.is_empty()
            && GuardFrameCoverage::new(self.then_tails.clone()).covers()
            && GuardFrameCoverage::new(self.else_tails.clone()).covers()
    }
}
