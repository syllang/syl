use syl_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct EirOrigin {
    span: Span,
    expansion_stack: Vec<EirExpansion>,
}

impl EirOrigin {
    pub(crate) fn new(span: Span, expansion_stack: Vec<EirExpansion>) -> Self {
        Self {
            span,
            expansion_stack,
        }
    }

    pub(crate) fn span(&self) -> Span {
        self.span
    }

    pub(crate) fn expansion_stack(&self) -> &[EirExpansion] {
        &self.expansion_stack
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct EirExpansion {
    callable: String,
    instance: String,
    span: Span,
}

impl EirExpansion {
    pub(crate) fn new(
        callable: impl Into<String>,
        instance: impl Into<String>,
        span: Span,
    ) -> Self {
        Self {
            callable: callable.into(),
            instance: instance.into(),
            span,
        }
    }

    pub(crate) fn callable(&self) -> &str {
        &self.callable
    }

    pub(crate) fn instance(&self) -> &str {
        &self.instance
    }

    pub(crate) fn span(&self) -> Span {
        self.span
    }
}
