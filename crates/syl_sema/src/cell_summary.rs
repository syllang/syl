use crate::{CompileError, DriverError};
use std::collections::BTreeMap;
use strum_macros::IntoStaticStr;
use syl_span::{SourceId, Span};

mod internal {
    use super::HwOrigin;
    use syl_span::Span;

    pub(super) fn origin_from_span(span: Span) -> HwOrigin {
        HwOrigin::new(span.source, span.start, span.end, Vec::new())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HwOrigin {
    source: SourceId,
    start: usize,
    end: usize,
    labels: Vec<String>,
}

impl HwOrigin {
    pub fn new(source: SourceId, start: usize, end: usize, labels: Vec<String>) -> Self {
        Self {
            source,
            start,
            end,
            labels,
        }
    }

    pub fn source(&self) -> SourceId {
        self.source
    }

    pub fn span_start(&self) -> usize {
        self.start
    }

    pub fn span_end(&self) -> usize {
        self.end
    }

    pub fn labels(&self) -> &[String] {
        &self.labels
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HwPlace {
    Ident(String),
}

/// A first-class cell boundary summary that can be reused by analyzed, std, precompiled, and opaque
/// cells.
///
/// Callers can materialize boundary summaries from external declarations via
/// [`CellSummaryRegistry`]; otherwise missing boundaries must still be rejected.
///
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CellSummary {
    callable: String,
    instance: String,
    drives: Vec<HwPlace>,
    reads: Vec<HwPlace>,
    creates: Vec<String>,
    origin: HwOrigin,
}

impl CellSummary {
    pub fn callable(&self) -> &str {
        &self.callable
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    pub fn drives(&self) -> &[HwPlace] {
        &self.drives
    }

    pub fn reads(&self) -> &[HwPlace] {
        &self.reads
    }

    pub fn creates(&self) -> &[String] {
        &self.creates
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CellInstanceMatch {
    Exact(String),
    Any,
}

impl CellInstanceMatch {
    pub fn exact(instance: impl Into<String>) -> Self {
        Self::Exact(instance.into())
    }

    pub fn any() -> Self {
        Self::Any
    }

    pub fn matches(&self, instance: &str) -> bool {
        match self {
            Self::Exact(expected) => expected == instance,
            Self::Any => true,
        }
    }

    pub fn exact_instance(&self) -> Option<&str> {
        match self {
            Self::Exact(instance) => Some(instance.as_str()),
            Self::Any => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CellSummaryDeclaration {
    callable: String,
    instance: CellInstanceMatch,
    drives: Vec<HwPlace>,
    reads: Vec<HwPlace>,
    creates: Vec<String>,
    origin: HwOrigin,
}

impl CellSummaryDeclaration {
    pub fn exact(
        callable: impl Into<String>,
        instance: impl Into<String>,
        origin: HwOrigin,
    ) -> Self {
        Self {
            callable: callable.into(),
            instance: CellInstanceMatch::exact(instance),
            drives: Vec::new(),
            reads: Vec::new(),
            creates: Vec::new(),
            origin,
        }
    }

    pub fn any_instance(callable: impl Into<String>, origin: HwOrigin) -> Self {
        Self {
            callable: callable.into(),
            instance: CellInstanceMatch::any(),
            drives: Vec::new(),
            reads: Vec::new(),
            creates: Vec::new(),
            origin,
        }
    }

    pub fn exact_at_span(
        callable: impl Into<String>,
        instance: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::exact(callable, instance, internal::origin_from_span(span))
    }

    pub fn any_instance_at_span(callable: impl Into<String>, span: Span) -> Self {
        Self::any_instance(callable, internal::origin_from_span(span))
    }

    pub fn callable(&self) -> &str {
        &self.callable
    }

    pub fn instance_match(&self) -> &CellInstanceMatch {
        &self.instance
    }

    pub fn instance(&self) -> Option<&str> {
        self.instance.exact_instance()
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }

    pub fn drives(&self) -> &[HwPlace] {
        &self.drives
    }

    pub fn reads(&self) -> &[HwPlace] {
        &self.reads
    }

    pub fn creates(&self) -> &[String] {
        &self.creates
    }

    pub fn add_drive(&mut self, place: HwPlace) {
        if !self.drives.contains(&place) {
            self.drives.push(place);
        }
    }

    pub fn add_read(&mut self, place: HwPlace) {
        if !self.reads.contains(&place) {
            self.reads.push(place);
        }
    }

    pub fn add_create(&mut self, name: impl Into<String>) {
        let name = name.into();
        if !self.creates.contains(&name) {
            self.creates.push(name);
        }
    }

    pub fn available_summary(&self, instance: &str) -> Option<CellSummary> {
        if !self.instance.matches(instance) {
            return None;
        }
        Some(CellSummary {
            callable: self.callable.clone(),
            instance: instance.to_string(),
            drives: self.drives.clone(),
            reads: self.reads.clone(),
            creates: self.creates.clone(),
            origin: self.origin.clone(),
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CellSummaryRegistry {
    summaries: BTreeMap<String, Vec<CellSummaryDeclaration>>,
}

impl CellSummaryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_empty(&self) -> bool {
        self.summaries.is_empty()
    }

    pub fn register(&mut self, declaration: CellSummaryDeclaration) {
        self.summaries
            .entry(declaration.callable().to_string())
            .or_default()
            .push(declaration);
    }

    pub fn resolve(&self, callable: &str, instance: &str) -> Option<CellSummary> {
        let declarations = self.summaries.get(callable)?;
        declarations
            .iter()
            .filter(|declaration| matches!(declaration.instance_match(), CellInstanceMatch::Exact(expected) if expected == instance))
            .chain(
                declarations
                    .iter()
                    .filter(|declaration| matches!(declaration.instance_match(), CellInstanceMatch::Any)),
            )
            .find_map(|declaration| declaration.available_summary(instance))
    }

    pub fn resolve_boundary(&self, boundary: &CellBoundarySummary) -> CellBoundarySummary {
        match boundary {
            CellBoundarySummary::Available(_) => boundary.clone(),
            CellBoundarySummary::Missing(summary) | CellBoundarySummary::UnsafeAssumed(summary) => {
                self.resolve(summary.callable(), summary.instance())
                    .map(CellBoundarySummary::from)
                    .unwrap_or_else(|| boundary.clone())
            }
        }
    }
}

impl Extend<CellSummaryDeclaration> for CellSummaryRegistry {
    fn extend<T: IntoIterator<Item = CellSummaryDeclaration>>(&mut self, iter: T) {
        for declaration in iter {
            self.register(declaration);
        }
    }
}

impl FromIterator<CellSummaryDeclaration> for CellSummaryRegistry {
    fn from_iter<T: IntoIterator<Item = CellSummaryDeclaration>>(iter: T) -> Self {
        let mut registry = Self::new();
        registry.extend(iter);
        registry
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum CellSummaryStatus {
    #[strum(serialize = "available")]
    Available,
    #[strum(serialize = "missing")]
    Missing,
    #[strum(serialize = "unsafe-assumed")]
    UnsafeAssumed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpaqueCellSummary {
    callable: String,
    instance: String,
    origin: HwOrigin,
}

impl OpaqueCellSummary {
    pub fn new(callable: impl Into<String>, instance: impl Into<String>, origin: HwOrigin) -> Self {
        Self {
            callable: callable.into(),
            instance: instance.into(),
            origin,
        }
    }

    pub fn callable(&self) -> &str {
        &self.callable
    }

    pub fn instance(&self) -> &str {
        &self.instance
    }

    pub fn origin(&self) -> &HwOrigin {
        &self.origin
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CellBoundarySummary {
    Available(CellSummary),
    Missing(OpaqueCellSummary),
    UnsafeAssumed(OpaqueCellSummary),
}

impl CellBoundarySummary {
    pub(crate) fn available(summary: CellSummary) -> Self {
        Self::Available(summary)
    }

    pub fn missing(
        callable: impl Into<String>,
        instance: impl Into<String>,
        origin: HwOrigin,
    ) -> Self {
        Self::Missing(OpaqueCellSummary::new(callable, instance, origin))
    }

    pub fn missing_at_span(
        callable: impl Into<String>,
        instance: impl Into<String>,
        span: Span,
    ) -> Self {
        Self::missing(callable, instance, internal::origin_from_span(span))
    }

    pub fn status(&self) -> CellSummaryStatus {
        match self {
            Self::Available(_) => CellSummaryStatus::Available,
            Self::Missing(_) => CellSummaryStatus::Missing,
            Self::UnsafeAssumed(_) => CellSummaryStatus::UnsafeAssumed,
        }
    }

    pub fn available_summary(&self) -> Option<&CellSummary> {
        match self {
            Self::Available(summary) => Some(summary),
            Self::Missing(_) | Self::UnsafeAssumed(_) => None,
        }
    }

    pub fn resolve_with(&self, registry: &CellSummaryRegistry) -> Self {
        registry.resolve_boundary(self)
    }

    pub fn require_available(&self) -> Result<&CellSummary, CompileError> {
        if let Some(summary) = self.available_summary() {
            return Ok(summary);
        }
        let opaque = match self {
            Self::Available(_) => unreachable!("available summary returned above"),
            Self::Missing(summary) | Self::UnsafeAssumed(summary) => summary,
        };
        let status = <&'static str>::from(self.status()).to_string();
        Err(CompileError::lowering_at(
            DriverError::MissingCellSummary {
                callable: opaque.callable().to_string(),
                instance: opaque.instance().to_string(),
                status,
            },
            Span::new_in(
                opaque.origin().source(),
                opaque.origin().span_start(),
                opaque.origin().span_end(),
            ),
        ))
    }
}

impl From<CellSummary> for CellBoundarySummary {
    fn from(summary: CellSummary) -> Self {
        Self::available(summary)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syl_span::{SourceId, Span};

    #[test]
    fn resolves_boundary_with_registered_declaration() {
        let summary_origin = HwOrigin::new(SourceId::new(7), 100, 120, Vec::new());
        let boundary_origin = HwOrigin::new(SourceId::new(8), 200, 220, Vec::new());
        let mut declaration =
            CellSummaryDeclaration::exact("VendorCell", "u_vendor", summary_origin.clone());
        declaration.add_drive(HwPlace::Ident("u_vendor.out".to_string()));
        declaration.add_read(HwPlace::Ident("u_vendor.in".to_string()));
        declaration.add_create("u_vendor_state");

        let registry = CellSummaryRegistry::from_iter([declaration]);
        let boundary = CellBoundarySummary::missing("VendorCell", "u_vendor", boundary_origin);

        let resolved = boundary.resolve_with(&registry);
        let summary = resolved
            .available_summary()
            .expect("registered summary must resolve boundary");

        assert_eq!(resolved.status(), CellSummaryStatus::Available);
        assert_eq!(summary.callable(), "VendorCell");
        assert_eq!(summary.instance(), "u_vendor");
        assert_eq!(summary.origin(), &summary_origin);
        assert_eq!(
            summary.drives(),
            &[HwPlace::Ident("u_vendor.out".to_string())]
        );
        assert_eq!(
            summary.reads(),
            &[HwPlace::Ident("u_vendor.in".to_string())]
        );
        assert_eq!(summary.creates(), &["u_vendor_state".to_string()]);
    }

    #[test]
    fn leaves_boundary_missing_without_matching_summary() {
        let mut declaration = CellSummaryDeclaration::exact(
            "VendorCell",
            "u_other",
            HwOrigin::new(SourceId::new(9), 300, 320, Vec::new()),
        );
        declaration.add_drive(HwPlace::Ident("u_other.out".to_string()));
        let registry = CellSummaryRegistry::from_iter([declaration]);
        let boundary = CellBoundarySummary::missing(
            "VendorCell",
            "u_vendor",
            HwOrigin::new(SourceId::new(10), 330, 340, Vec::new()),
        );

        let resolved = boundary.resolve_with(&registry);

        assert_eq!(resolved.status(), CellSummaryStatus::Missing);
        assert!(resolved.available_summary().is_none());
        assert_eq!(
            resolved.require_available().unwrap_err().diagnostic().span,
            Span::new_in(SourceId::new(10), 330, 340)
        );
    }
}
