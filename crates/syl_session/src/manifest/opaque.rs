//! Private versioned serialization boundary for sema-owned opaque metadata.
mod types;

use crate::{ProjectError, Vfs};
use serde::Deserialize;
use std::path::Path;
use syl_sema::{
    BackendConstraint, OpaqueItemKind, OpaqueItemSummary, OpaqueSummaryTable,
    SummaryDomainBehavior, SummaryEndpoint, SummaryLatencyClass, SummaryPath,
    SummaryProtocolPreservation, TrustBoundary,
};
use types::{RawCapability, RawDirection, RawLayout, RawProtocol};

pub(super) fn load<V: Vfs>(path: &Path, vfs: &V) -> Result<OpaqueSummaryTable, ProjectError> {
    let invalid = |message| ProjectError::OpaqueSummaryInvalid {
        path: path.to_path_buf(),
        message,
    };
    let extension = path.extension().and_then(|ext| ext.to_str());
    if !matches!(extension, Some("json" | "toml")) {
        return Err(invalid("expected a .json or .toml summary file".into()));
    }
    let text = vfs.read_to_string(path)?;
    let raw: RawDocument = match extension {
        Some("json") => serde_json::from_str(&text).map_err(|err| err.to_string()),
        _ => toml::from_str(&text).map_err(|err| err.to_string()),
    }
    .map_err(|message| ProjectError::OpaqueSummaryParse {
        path: path.to_path_buf(),
        message,
    })?;
    if raw.schema_version != 1 {
        return Err(invalid(format!(
            "unsupported schema_version {}; expected 1",
            raw.schema_version
        )));
    }
    raw.summaries
        .into_iter()
        .map(RawSummary::convert)
        .collect::<Result<_, _>>()
        .map_err(invalid)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawDocument {
    schema_version: u32,
    #[serde(default)]
    summaries: Vec<RawSummary>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawSummary {
    kind: RawKind,
    callable: String,
    #[serde(default)]
    endpoints: Vec<RawEndpoint>,
    #[serde(default)]
    driven_fields: Vec<String>,
    #[serde(default)]
    consumed_fields: Vec<String>,
    #[serde(default)]
    domain_behavior: RawDomainBehavior,
    #[serde(default)]
    latency_class: RawLatency,
    #[serde(default)]
    protocol_preservation: RawPreservation,
    #[serde(default)]
    trust_boundary: RawTrust,
    #[serde(default)]
    backend_constraints: Vec<RawConstraint>,
}

impl RawSummary {
    fn convert(self) -> Result<OpaqueItemSummary, String> {
        nonblank(&self.callable, "callable")?;
        Ok(OpaqueItemSummary::builder(self.kind.into(), self.callable)
            .endpoints(
                self.endpoints
                    .into_iter()
                    .map(RawEndpoint::convert)
                    .collect::<Result<_, _>>()?,
            )
            .driven_fields(
                self.driven_fields
                    .into_iter()
                    .map(summary_path)
                    .collect::<Result<_, _>>()?,
            )
            .consumed_fields(
                self.consumed_fields
                    .into_iter()
                    .map(summary_path)
                    .collect::<Result<_, _>>()?,
            )
            .domain_behavior(self.domain_behavior.into())
            .latency_class(self.latency_class.into())
            .protocol_preservation(self.protocol_preservation.into())
            .trust_boundary(self.trust_boundary.into())
            .backend_constraints(
                self.backend_constraints
                    .into_iter()
                    .map(Into::into)
                    .collect(),
            )
            .build())
    }
}

fn nonblank(value: &str, field: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        Err(format!("{field} must not be blank"))
    } else {
        Ok(())
    }
}

fn summary_path(value: String) -> Result<SummaryPath, String> {
    let mut parts = value.split('.');
    let root = parts.next().unwrap_or_default();
    nonblank(root, "summary path segment")?;
    let mut path = SummaryPath::new(root);
    for part in parts {
        nonblank(part, "summary path segment")?;
        path = path.with_field(part);
    }
    Ok(path)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawEndpoint {
    name: String,
    direction: RawDirection,
    layout: RawLayout,
    capability: RawCapability,
    #[serde(default)]
    protocol: Option<RawProtocol>,
}

impl RawEndpoint {
    fn convert(self) -> Result<SummaryEndpoint, String> {
        nonblank(&self.name, "endpoint.name")?;
        let mut endpoint = SummaryEndpoint::new(
            self.name,
            self.direction.into(),
            self.layout.into(),
            self.capability.into(),
        );
        if let Some(protocol) = self.protocol {
            endpoint = endpoint.with_protocol(protocol.into());
        }
        Ok(endpoint)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawKind {
    #[serde(rename = "source_cell")]
    Source,
    #[serde(rename = "extern_cell")]
    Extern,
    #[serde(rename = "precompiled_cell")]
    Precompiled,
}
impl From<RawKind> for OpaqueItemKind {
    fn from(value: RawKind) -> Self {
        match value {
            RawKind::Source => Self::SourceCell,
            RawKind::Extern => Self::ExternCell,
            RawKind::Precompiled => Self::PrecompiledCell,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawLatency {
    #[default]
    Unknown,
    Transparent,
    Sequential,
}
impl From<RawLatency> for SummaryLatencyClass {
    fn from(value: RawLatency) -> Self {
        match value {
            RawLatency::Unknown => Self::Unknown,
            RawLatency::Transparent => Self::Transparent,
            RawLatency::Sequential => Self::Sequential,
        }
    }
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "snake_case")]
enum RawPreservation {
    #[default]
    Unknown,
    Preserved,
    Opaque,
}
impl From<RawPreservation> for SummaryProtocolPreservation {
    fn from(value: RawPreservation) -> Self {
        match value {
            RawPreservation::Unknown => Self::Unknown,
            RawPreservation::Preserved => Self::Preserved,
            RawPreservation::Opaque => Self::Opaque,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawDomainBehavior {
    Simple(SimpleBehavior),
    Explicit(ExplicitBehavior),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SimpleBehavior {
    Unknown,
    Clockless,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum ExplicitBehavior {
    Explicit {
        clock_inputs: Vec<String>,
        reset_inputs: Vec<String>,
    },
}
impl Default for RawDomainBehavior {
    fn default() -> Self {
        Self::Simple(SimpleBehavior::Unknown)
    }
}
impl From<RawDomainBehavior> for SummaryDomainBehavior {
    fn from(value: RawDomainBehavior) -> Self {
        match value {
            RawDomainBehavior::Simple(SimpleBehavior::Unknown) => Self::Unknown,
            RawDomainBehavior::Simple(SimpleBehavior::Clockless) => Self::Clockless,
            RawDomainBehavior::Explicit(ExplicitBehavior::Explicit {
                clock_inputs,
                reset_inputs,
            }) => Self::Explicit {
                clock_inputs,
                reset_inputs,
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawTrust {
    Simple(SimpleTrust),
    Detailed(DetailedTrust),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SimpleTrust {
    SourceDerived,
    TrustedPrecompiled,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DetailedTrust {
    VendorBlackBox { vendor: String },
    UnsafeBlackBox { rationale: String },
}
impl Default for RawTrust {
    fn default() -> Self {
        Self::Simple(SimpleTrust::SourceDerived)
    }
}
impl From<RawTrust> for TrustBoundary {
    fn from(value: RawTrust) -> Self {
        match value {
            RawTrust::Simple(SimpleTrust::SourceDerived) => Self::SourceDerived,
            RawTrust::Simple(SimpleTrust::TrustedPrecompiled) => Self::TrustedPrecompiled,
            RawTrust::Detailed(DetailedTrust::VendorBlackBox { vendor }) => {
                Self::VendorBlackBox { vendor }
            }
            RawTrust::Detailed(DetailedTrust::UnsafeBlackBox { rationale }) => {
                Self::UnsafeBlackBox { rationale }
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawConstraint {
    Simple(SimpleConstraint),
    Detailed(DetailedConstraint),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum SimpleConstraint {
    ForbidsRetiming,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum DetailedConstraint {
    RequiresBackend { backend: String },
    RequiresBlackBoxArtifact { artifact: String },
    ForbidsRetiming,
}
impl From<RawConstraint> for BackendConstraint {
    fn from(value: RawConstraint) -> Self {
        match value {
            RawConstraint::Simple(SimpleConstraint::ForbidsRetiming)
            | RawConstraint::Detailed(DetailedConstraint::ForbidsRetiming) => Self::ForbidsRetiming,
            RawConstraint::Detailed(DetailedConstraint::RequiresBackend { backend }) => {
                Self::RequiresBackend { backend }
            }
            RawConstraint::Detailed(DetailedConstraint::RequiresBlackBoxArtifact { artifact }) => {
                Self::RequiresBlackBoxArtifact { artifact }
            }
        }
    }
}
