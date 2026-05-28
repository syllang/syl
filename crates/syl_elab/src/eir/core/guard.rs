use super::EirBound;
use syl_span::Span;

/// Stack of scope frames that guard whether a hardware signal is active.
///
/// Guards form a **stack** (innermost frame last). Two guards are mutually
/// exclusive if they share the same prefix of frames and the first differing
/// frame is an `IfThen`/`IfElse` opposite pair with matching labels.
///
/// **How mutual exclusion works:**
/// - `[]` (root) — always active, never exclusive.
/// - `[IfThen("lbl")]` vs `[IfElse("lbl")]` — exclusive (same label, opposite branch).
/// - `[IfThen("a"), IfThen("b")]` vs `[IfThen("a"), IfElse("b")]` — exclusive
///   (nested under same outer `a`, opposite at level `b`).
/// - `[IfThen("a")]` vs `[IfThen("b")]` — **not** exclusive (different labels,
///   could come from different scopes entirely).
/// - `[IfThen("a")]` vs `[IfThen("a"), IfThen("b")]` — **not** exclusive
///   (one is a prefix of the other; the inner frame is *inside* the outer).
///
/// **Loop frames never participate in mutual-exclusion checks.**
/// Two loop frames with the same label are treated as potentially overlapping.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct EirGuard {
    frames: Vec<EirGuardFrame>,
}

impl EirGuard {
    pub(crate) fn root() -> Self {
        Self { frames: Vec::new() }
    }

    pub(crate) fn from_frames(frames: &[EirGuardFrame]) -> Self {
        Self {
            frames: frames.to_vec(),
        }
    }

    pub(crate) fn frames(&self) -> &[EirGuardFrame] {
        &self.frames
    }

    pub(crate) fn is_root(&self) -> bool {
        self.frames.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) struct EirGuardLabel {
    display: String,
    span: Span,
}

impl EirGuardLabel {
    fn new(display: impl Into<String>, span: Span) -> Self {
        Self {
            display: display.into(),
            span,
        }
    }

    pub(crate) fn display(&self) -> &str {
        &self.display
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum EirGuardFrame {
    IfThen {
        label: EirGuardLabel,
    },
    IfElse {
        label: EirGuardLabel,
    },
    Loop {
        label: EirGuardLabel,
        index: String,
        start: EirBound,
        end: EirBound,
    },
}

impl EirGuardFrame {
    /// Creates a guard frame for the `then` branch of an if-statement.
    pub(crate) fn if_then(label: impl Into<String>, span: Span) -> Self {
        Self::IfThen {
            label: EirGuardLabel::new(label, span),
        }
    }

    /// Creates a guard frame for the `else` branch of an if-statement.
    pub(crate) fn if_else(label: impl Into<String>, span: Span) -> Self {
        Self::IfElse {
            label: EirGuardLabel::new(label, span),
        }
    }

    /// Creates a guard frame for a loop body.
    pub(crate) fn loop_frame(
        label: impl Into<String>,
        index: impl Into<String>,
        start: impl Into<EirBound>,
        end: impl Into<EirBound>,
        span: Span,
    ) -> Self {
        Self::Loop {
            label: EirGuardLabel::new(label, span),
            index: index.into(),
            start: start.into(),
            end: end.into(),
        }
    }

    /// Returns `true` if `self` and `other` are opposite branches of the same if.
    ///
    /// Only `IfThen`/`IfElse` pairs with matching labels are opposites.
    /// `Loop` frames never produce mutual exclusion.
    pub(crate) fn is_opposite_if_branch(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IfThen { label: left }, Self::IfElse { label: right })
            | (Self::IfElse { label: left }, Self::IfThen { label: right }) => left == right,
            _ => false,
        }
    }
}
