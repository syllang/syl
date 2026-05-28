use crate::DefId;
use syl_span::Span;

/// Composite key identifying an enum variant by its parent definition and name.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct HirEnumVariantKey {
    pub enum_def: DefId,
    pub name: String,
}

/// A resolved enum variant with its concrete numeric value.
#[derive(Clone)]
#[non_exhaustive]
pub struct HirEnumVariant {
    pub enum_def: DefId,
    pub name: String,
    pub value: u64,
    pub span: Span,
}

impl HirEnumVariantKey {
    /// Creates a new variant key.
    pub fn new(enum_def: DefId, name: impl Into<String>) -> Self {
        Self {
            enum_def,
            name: name.into(),
        }
    }
}

impl HirEnumVariant {
    /// Creates a new variant with a resolved numeric value.
    pub fn new(enum_def: DefId, name: impl Into<String>, value: u64, span: Span) -> Self {
        Self {
            enum_def,
            name: name.into(),
            value,
            span,
        }
    }

    pub(crate) fn summary_count(&self) -> usize {
        self.enum_def.get() + self.name.len() + self.span.start
    }
}
