use crate::{
    facts::{CapabilityKind, DomainFact},
    hir::HirPortDirection,
    tir::{TirConstTerm, TirDesign},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum OpaqueItemKind {
    SourceCell,
    ExternModule,
    PrecompiledCell,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SummaryPath {
    root: String,
    fields: Vec<String>,
}

impl SummaryPath {
    pub fn new(root: impl Into<String>) -> Self {
        Self {
            root: root.into(),
            fields: Vec::new(),
        }
    }

    pub fn with_field(mut self, field: impl Into<String>) -> Self {
        self.fields.push(field.into());
        self
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn display(&self) -> String {
        if self.fields.is_empty() {
            self.root.clone()
        } else {
            format!("{}.{}", self.root, self.fields.join("."))
        }
    }

    pub fn flattened(&self) -> String {
        if self.fields.is_empty() {
            self.root.clone()
        } else {
            format!("{}_{}", self.root, self.fields.join("_"))
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryDirection {
    In,
    InOut,
    Out,
}

impl From<HirPortDirection> for SummaryDirection {
    fn from(direction: HirPortDirection) -> Self {
        match direction {
            HirPortDirection::In => Self::In,
            HirPortDirection::InOut => Self::InOut,
            HirPortDirection::Out => Self::Out,
            _ => Self::In,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryFieldDirection {
    In,
    InOut,
    Out,
}

impl From<&crate::facts::ProtocolFieldDirection> for SummaryFieldDirection {
    fn from(direction: &crate::facts::ProtocolFieldDirection) -> Self {
        match direction {
            crate::facts::ProtocolFieldDirection::In => Self::In,
            crate::facts::ProtocolFieldDirection::InOut => Self::InOut,
            crate::facts::ProtocolFieldDirection::Out => Self::Out,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SummaryViewField {
    name: String,
    direction: SummaryFieldDirection,
}

impl SummaryViewField {
    pub fn new(name: impl Into<String>, direction: SummaryFieldDirection) -> Self {
        Self {
            name: name.into(),
            direction,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn direction(&self) -> SummaryFieldDirection {
        self.direction
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SummaryView {
    name: String,
    fields: Vec<SummaryViewField>,
}

impl SummaryView {
    pub fn new(name: impl Into<String>, fields: Vec<SummaryViewField>) -> Self {
        Self {
            name: name.into(),
            fields,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[SummaryViewField] {
        &self.fields
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SummaryProtocol {
    name: String,
    fields: Vec<String>,
    views: Vec<SummaryView>,
}

impl SummaryProtocol {
    pub fn new(name: impl Into<String>, fields: Vec<String>, views: Vec<SummaryView>) -> Self {
        Self {
            name: name.into(),
            fields,
            views,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn views(&self) -> &[SummaryView] {
        &self.views
    }
}

impl From<&crate::facts::ProtocolSummary> for SummaryProtocol {
    fn from(summary: &crate::facts::ProtocolSummary) -> Self {
        Self::new(
            summary.name(),
            summary.fields().to_vec(),
            summary
                .views()
                .iter()
                .map(|view| {
                    SummaryView::new(
                        view.name(),
                        view.fields()
                            .iter()
                            .map(|field| {
                                SummaryViewField::new(field.name(), field.direction().into())
                            })
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryLayoutConst {
    Known(u64),
    Symbol(String),
    Unknown,
}

impl From<&TirConstTerm> for SummaryLayoutConst {
    fn from(term: &TirConstTerm) -> Self {
        match term {
            TirConstTerm::NatLiteral(value) => Self::Known(*value),
            TirConstTerm::Named { name, .. } => Self::Symbol(name.clone()),
            TirConstTerm::Expr { label } => Self::Symbol(label.clone()),
            TirConstTerm::Unknown | TirConstTerm::BoolLiteral(_) => Self::Unknown,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryWordEncoding {
    UInt,
    Bits,
    SInt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryLayout {
    Unknown,
    Nat,
    Bit,
    Bool,
    Domain,
    Clock,
    Reset,
    Str,
    Word {
        encoding: SummaryWordEncoding,
        width: SummaryLayoutConst,
    },
    Array {
        len: SummaryLayoutConst,
        elem: Box<SummaryLayout>,
    },
    Aggregate {
        name: String,
        fields: Vec<String>,
    },
    Enum {
        name: String,
        variants: Vec<String>,
    },
    View {
        protocol: String,
        view: String,
        fields: Vec<String>,
    },
    Opaque {
        label: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryDomain {
    Named(String),
    Builtin,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryCapability {
    Unknown,
    Value,
    Domain,
    Clock {
        domain: SummaryDomain,
    },
    Reset {
        domain: SummaryDomain,
    },
    View {
        view: String,
        readable_fields: Vec<String>,
        writable_fields: Vec<String>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SummaryEndpoint {
    name: String,
    direction: SummaryDirection,
    layout: SummaryLayout,
    capability: SummaryCapability,
    protocol: Option<SummaryProtocol>,
}

impl SummaryEndpoint {
    pub fn new(
        name: impl Into<String>,
        direction: SummaryDirection,
        layout: SummaryLayout,
        capability: SummaryCapability,
    ) -> Self {
        Self {
            name: name.into(),
            direction,
            layout,
            capability,
            protocol: None,
        }
    }

    pub fn with_protocol(mut self, protocol: SummaryProtocol) -> Self {
        self.protocol = Some(protocol);
        self
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn direction(&self) -> SummaryDirection {
        self.direction
    }

    pub fn layout(&self) -> &SummaryLayout {
        &self.layout
    }

    pub fn capability(&self) -> &SummaryCapability {
        &self.capability
    }

    pub fn protocol(&self) -> Option<&SummaryProtocol> {
        self.protocol.as_ref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryDomainBehavior {
    Unknown,
    Clockless,
    Explicit {
        clock_inputs: Vec<String>,
        reset_inputs: Vec<String>,
    },
}

impl SummaryDomainBehavior {
    pub(crate) fn is_unknown(&self) -> bool {
        matches!(self, Self::Unknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryLatencyClass {
    Unknown,
    Transparent,
    Sequential,
}

impl SummaryLatencyClass {
    pub(crate) fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SummaryProtocolPreservation {
    Unknown,
    Preserved,
    Opaque,
}

impl SummaryProtocolPreservation {
    pub(crate) fn is_unknown(self) -> bool {
        matches!(self, Self::Unknown)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BackendConstraint {
    RequiresBackend { backend: String },
    RequiresBlackBoxArtifact { artifact: String },
    ForbidsRetiming,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TrustBoundary {
    SourceDerived,
    TrustedPrecompiled,
    VendorBlackBox { vendor: String },
    UnsafeBlackBox { rationale: String },
}

impl TrustBoundary {
    pub(crate) fn is_source_derived(&self) -> bool {
        matches!(self, Self::SourceDerived)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpaqueItemSummary {
    kind: OpaqueItemKind,
    callable: String,
    endpoints: Vec<SummaryEndpoint>,
    driven_fields: Vec<SummaryPath>,
    consumed_fields: Vec<SummaryPath>,
    domain_behavior: SummaryDomainBehavior,
    latency_class: SummaryLatencyClass,
    protocol_preservation: SummaryProtocolPreservation,
    trust_boundary: TrustBoundary,
    backend_constraints: Vec<BackendConstraint>,
}

impl OpaqueItemSummary {
    pub fn builder(kind: OpaqueItemKind, callable: impl Into<String>) -> OpaqueItemSummaryBuilder {
        OpaqueItemSummaryBuilder {
            kind,
            callable: callable.into(),
            endpoints: Vec::new(),
            driven_fields: Vec::new(),
            consumed_fields: Vec::new(),
            domain_behavior: SummaryDomainBehavior::Unknown,
            latency_class: SummaryLatencyClass::Unknown,
            protocol_preservation: SummaryProtocolPreservation::Unknown,
            trust_boundary: TrustBoundary::SourceDerived,
            backend_constraints: Vec::new(),
        }
    }

    pub fn kind(&self) -> OpaqueItemKind {
        self.kind
    }

    pub fn callable(&self) -> &str {
        &self.callable
    }

    pub fn endpoints(&self) -> &[SummaryEndpoint] {
        &self.endpoints
    }

    pub fn driven_fields(&self) -> &[SummaryPath] {
        &self.driven_fields
    }

    pub fn consumed_fields(&self) -> &[SummaryPath] {
        &self.consumed_fields
    }

    pub fn domain_behavior(&self) -> &SummaryDomainBehavior {
        &self.domain_behavior
    }

    pub fn latency_class(&self) -> SummaryLatencyClass {
        self.latency_class
    }

    pub fn protocol_preservation(&self) -> SummaryProtocolPreservation {
        self.protocol_preservation
    }

    pub fn trust_boundary(&self) -> &TrustBoundary {
        &self.trust_boundary
    }

    pub fn backend_constraints(&self) -> &[BackendConstraint] {
        &self.backend_constraints
    }

    pub(crate) fn merged_with(&self, overlay: &Self) -> Self {
        let mut backend_constraints = self.backend_constraints.clone();
        backend_constraints.extend(overlay.backend_constraints.clone());
        Self {
            kind: overlay.kind,
            callable: self.callable.clone(),
            endpoints: if overlay.endpoints.is_empty() {
                self.endpoints.clone()
            } else {
                overlay.endpoints.clone()
            },
            driven_fields: if overlay.driven_fields.is_empty() {
                self.driven_fields.clone()
            } else {
                overlay.driven_fields.clone()
            },
            consumed_fields: if overlay.consumed_fields.is_empty() {
                self.consumed_fields.clone()
            } else {
                overlay.consumed_fields.clone()
            },
            domain_behavior: if overlay.domain_behavior.is_unknown() {
                self.domain_behavior.clone()
            } else {
                overlay.domain_behavior.clone()
            },
            latency_class: if overlay.latency_class.is_unknown() {
                self.latency_class
            } else {
                overlay.latency_class
            },
            protocol_preservation: if overlay.protocol_preservation.is_unknown() {
                self.protocol_preservation
            } else {
                overlay.protocol_preservation
            },
            trust_boundary: if overlay.trust_boundary.is_source_derived() {
                self.trust_boundary.clone()
            } else {
                overlay.trust_boundary.clone()
            },
            backend_constraints,
        }
    }

    pub(crate) fn summary_capability_for_kind(
        tir: &TirDesign,
        kind: &CapabilityKind,
    ) -> SummaryCapability {
        match kind {
            CapabilityKind::Value => SummaryCapability::Value,
            CapabilityKind::Domain => SummaryCapability::Domain,
            CapabilityKind::Clock { domain } => SummaryCapability::Clock {
                domain: summary_domain_for_fact(tir, domain),
            },
            CapabilityKind::Reset { domain } => SummaryCapability::Reset {
                domain: summary_domain_for_fact(tir, domain),
            },
            CapabilityKind::View(view) => SummaryCapability::View {
                view: view.view().to_string(),
                readable_fields: view.readable_fields().to_vec(),
                writable_fields: view.writable_fields().to_vec(),
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct OpaqueItemSummaryBuilder {
    kind: OpaqueItemKind,
    callable: String,
    endpoints: Vec<SummaryEndpoint>,
    driven_fields: Vec<SummaryPath>,
    consumed_fields: Vec<SummaryPath>,
    domain_behavior: SummaryDomainBehavior,
    latency_class: SummaryLatencyClass,
    protocol_preservation: SummaryProtocolPreservation,
    trust_boundary: TrustBoundary,
    backend_constraints: Vec<BackendConstraint>,
}

impl OpaqueItemSummaryBuilder {
    pub fn endpoint(mut self, endpoint: SummaryEndpoint) -> Self {
        self.endpoints.push(endpoint);
        self
    }

    pub fn endpoints(mut self, endpoints: Vec<SummaryEndpoint>) -> Self {
        self.endpoints = endpoints;
        self
    }

    pub fn driven_field(mut self, path: SummaryPath) -> Self {
        self.driven_fields.push(path);
        self
    }

    pub fn driven_fields(mut self, paths: Vec<SummaryPath>) -> Self {
        self.driven_fields = paths;
        self
    }

    pub fn consumed_field(mut self, path: SummaryPath) -> Self {
        self.consumed_fields.push(path);
        self
    }

    pub fn consumed_fields(mut self, paths: Vec<SummaryPath>) -> Self {
        self.consumed_fields = paths;
        self
    }

    pub fn domain_behavior(mut self, behavior: SummaryDomainBehavior) -> Self {
        self.domain_behavior = behavior;
        self
    }

    pub fn latency_class(mut self, latency_class: SummaryLatencyClass) -> Self {
        self.latency_class = latency_class;
        self
    }

    pub fn protocol_preservation(mut self, preservation: SummaryProtocolPreservation) -> Self {
        self.protocol_preservation = preservation;
        self
    }

    pub fn trust_boundary(mut self, boundary: TrustBoundary) -> Self {
        self.trust_boundary = boundary;
        self
    }

    pub fn backend_constraint(mut self, constraint: BackendConstraint) -> Self {
        self.backend_constraints.push(constraint);
        self
    }

    pub fn backend_constraints(mut self, constraints: Vec<BackendConstraint>) -> Self {
        self.backend_constraints = constraints;
        self
    }

    pub fn build(self) -> OpaqueItemSummary {
        OpaqueItemSummary {
            kind: self.kind,
            callable: self.callable,
            endpoints: self.endpoints,
            driven_fields: self.driven_fields,
            consumed_fields: self.consumed_fields,
            domain_behavior: self.domain_behavior,
            latency_class: self.latency_class,
            protocol_preservation: self.protocol_preservation,
            trust_boundary: self.trust_boundary,
            backend_constraints: self.backend_constraints,
        }
    }
}

fn summary_domain_for_fact(tir: &TirDesign, domain: &DomainFact) -> SummaryDomain {
    match domain {
        DomainFact::Named(type_id) => tir
            .type_table()
            .get(*type_id)
            .map(|ty| SummaryDomain::Named(ty.label()))
            .unwrap_or(SummaryDomain::Unknown),
        DomainFact::BuiltinDomain => SummaryDomain::Builtin,
        DomainFact::Unknown => SummaryDomain::Unknown,
    }
}
