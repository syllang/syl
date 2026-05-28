use syl_span::Span;

/// Source location origin for an EIR object, tracking how it was created.
///
/// The `expansion_stack` records each cell instantiation that produced this
/// object, from outermost (first) to innermost (last). This mirrors the
/// elaboration call-stack and helps produce useful error messages.
///
/// **Immutable sharing caveat:** Once constructed, the `expansion_stack` is
/// never modified. When the elaborator inserts a signal into the environment
/// via `env.insert`, the inserted `EirOrigin` is cloned by value — so two
/// signals created from different instantiation paths may have **identical**
/// origin stacks if they both originated from the same `env.insert` call.
/// Downstream code should not assume that equal origin stacks imply the
/// same instantiation path.
///
/// ```ignore
/// // Given env.insert("x", EirOrigin::new(span, [Expand("Adder", "a1")])):
/// // If later code does env.get("x") from a nested expansion
/// // [Expand("Mul", "m1"), Expand("Adder", "a1")],
/// // the origin STILL shows only [Expand("Adder", "a1")] — it's the
/// // cloned original, not extended with the outer expansion.
/// ```
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

/// One level of elaboration expansion — which cell was instantiated as what.
///
/// Each expansion records `callable` (the cell type, e.g. `"Adder"`) and
/// `instance` (the instance name, e.g. `"a1"`), plus the source span of
/// the instantiation site.
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
