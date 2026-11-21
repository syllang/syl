//! Layout, capability, and protocol DTOs; no serialization leaks into sema.
use serde::Deserialize;
use syl_sema::{
    SummaryCapability, SummaryDirection, SummaryDomain, SummaryFieldDirection, SummaryLayout,
    SummaryLayoutConst, SummaryProtocol, SummaryView, SummaryViewField, SummaryWordEncoding,
};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RawDirection {
    In,
    InOut,
    Out,
}
impl From<RawDirection> for SummaryDirection {
    fn from(value: RawDirection) -> Self {
        match value {
            RawDirection::In => Self::In,
            RawDirection::InOut => Self::InOut,
            RawDirection::Out => Self::Out,
        }
    }
}
impl From<RawDirection> for SummaryFieldDirection {
    fn from(value: RawDirection) -> Self {
        match value {
            RawDirection::In => Self::In,
            RawDirection::InOut => Self::InOut,
            RawDirection::Out => Self::Out,
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawLayout {
    Simple(SimpleLayout),
    Detailed(DetailedLayout),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SimpleLayout {
    Unknown,
    Nat,
    Bit,
    Bool,
    Domain,
    Clock,
    Reset,
    Str,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DetailedLayout {
    Word {
        encoding: RawEncoding,
        width: RawConst,
    },
    Array {
        len: RawConst,
        elem: Box<RawLayout>,
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
#[derive(Deserialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum RawEncoding {
    UInt,
    Bits,
    SInt,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawConst {
    Known(u64),
    Symbol(String),
    Unknown(()),
}
impl From<RawConst> for SummaryLayoutConst {
    fn from(value: RawConst) -> Self {
        match value {
            RawConst::Known(value) => Self::Known(value),
            RawConst::Symbol(value) if value != "unknown" => Self::Symbol(value),
            _ => Self::Unknown,
        }
    }
}
impl From<RawEncoding> for SummaryWordEncoding {
    fn from(value: RawEncoding) -> Self {
        match value {
            RawEncoding::UInt => Self::UInt,
            RawEncoding::Bits => Self::Bits,
            RawEncoding::SInt => Self::SInt,
        }
    }
}
impl From<RawLayout> for SummaryLayout {
    fn from(value: RawLayout) -> Self {
        match value {
            RawLayout::Simple(simple) => match simple {
                SimpleLayout::Unknown => Self::Unknown,
                SimpleLayout::Nat => Self::Nat,
                SimpleLayout::Bit => Self::Bit,
                SimpleLayout::Bool => Self::Bool,
                SimpleLayout::Domain => Self::Domain,
                SimpleLayout::Clock => Self::Clock,
                SimpleLayout::Reset => Self::Reset,
                SimpleLayout::Str => Self::Str,
            },
            RawLayout::Detailed(detailed) => match detailed {
                DetailedLayout::Word { encoding, width } => Self::Word {
                    encoding: encoding.into(),
                    width: width.into(),
                },
                DetailedLayout::Array { len, elem } => Self::Array {
                    len: len.into(),
                    elem: Box::new((*elem).into()),
                },
                DetailedLayout::Aggregate { name, fields } => Self::Aggregate { name, fields },
                DetailedLayout::Enum { name, variants } => Self::Enum { name, variants },
                DetailedLayout::View {
                    protocol,
                    view,
                    fields,
                } => Self::View {
                    protocol,
                    view,
                    fields,
                },
                DetailedLayout::Opaque { label } => Self::Opaque { label },
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawCapability {
    Simple(SimpleCapability),
    Detailed(DetailedCapability),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SimpleCapability {
    Unknown,
    Value,
    Domain,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum DetailedCapability {
    Clock {
        domain: RawDomain,
    },
    Reset {
        domain: RawDomain,
    },
    View {
        view: String,
        readable_fields: Vec<String>,
        writable_fields: Vec<String>,
    },
}
#[derive(Deserialize)]
#[serde(untagged)]
pub(super) enum RawDomain {
    Simple(SimpleDomain),
    Named(NamedDomain),
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SimpleDomain {
    Builtin,
    Unknown,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct NamedDomain {
    named: String,
}
impl From<RawDomain> for SummaryDomain {
    fn from(value: RawDomain) -> Self {
        match value {
            RawDomain::Simple(SimpleDomain::Builtin) => Self::Builtin,
            RawDomain::Simple(SimpleDomain::Unknown) => Self::Unknown,
            RawDomain::Named(NamedDomain { named }) => Self::Named(named),
        }
    }
}
impl From<RawCapability> for SummaryCapability {
    fn from(value: RawCapability) -> Self {
        match value {
            RawCapability::Simple(simple) => match simple {
                SimpleCapability::Unknown => Self::Unknown,
                SimpleCapability::Value => Self::Value,
                SimpleCapability::Domain => Self::Domain,
            },
            RawCapability::Detailed(detailed) => match detailed {
                DetailedCapability::Clock { domain } => Self::Clock {
                    domain: domain.into(),
                },
                DetailedCapability::Reset { domain } => Self::Reset {
                    domain: domain.into(),
                },
                DetailedCapability::View {
                    view,
                    readable_fields,
                    writable_fields,
                } => Self::View {
                    view,
                    readable_fields,
                    writable_fields,
                },
            },
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct RawProtocol {
    name: String,
    fields: Vec<String>,
    views: Vec<RawView>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawView {
    name: String,
    fields: Vec<RawViewField>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawViewField {
    name: String,
    direction: RawDirection,
}
impl From<RawProtocol> for SummaryProtocol {
    fn from(value: RawProtocol) -> Self {
        Self::new(
            value.name,
            value.fields,
            value
                .views
                .into_iter()
                .map(|view| {
                    SummaryView::new(
                        view.name,
                        view.fields
                            .into_iter()
                            .map(|field| SummaryViewField::new(field.name, field.direction.into()))
                            .collect(),
                    )
                })
                .collect(),
        )
    }
}
