use crate::eir_expr::EirBound;
use syl_span::Span;

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
    pub(crate) fn if_then(label: impl Into<String>, span: Span) -> Self {
        Self::IfThen {
            label: EirGuardLabel::new(label, span),
        }
    }

    pub(crate) fn if_else(label: impl Into<String>, span: Span) -> Self {
        Self::IfElse {
            label: EirGuardLabel::new(label, span),
        }
    }

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

    pub(crate) fn is_opposite_if_branch(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::IfThen { label: left }, Self::IfElse { label: right })
            | (Self::IfElse { label: left }, Self::IfThen { label: right }) => left == right,
            _ => false,
        }
    }
}
