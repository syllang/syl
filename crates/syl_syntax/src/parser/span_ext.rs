use crate::Pattern;

#[non_exhaustive]
pub(super) struct PatternSpan<'a> {
    pattern: &'a Pattern,
}

impl<'a> PatternSpan<'a> {
    pub(super) fn new(pattern: &'a Pattern) -> Self {
        Self { pattern }
    }

    pub(super) fn span(&self) -> syl_span::Span {
        self.pattern.span()
    }
}
