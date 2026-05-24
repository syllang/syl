use super::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryDomainBehavior,
    SummaryEndpoint, SummaryLatencyClass, SummaryLayout, SummaryPath, SummaryProtocol,
    SummaryProtocolPreservation, SummaryWordEncoding,
};
use crate::{
    facts::{CapabilityTable, HirFactId, ProtocolFacts, TypeTable},
    hir::{HirDefKind, HirSignatureResultBinding},
    tir::{TirDesign, TirType},
};

pub(super) fn collect_extern_summary(
    tir: &TirDesign,
    types: &TypeTable,
    capabilities: &CapabilityTable,
    protocols: &ProtocolFacts,
    item: &crate::hir::HirExternModuleItem,
) -> OpaqueItemSummary {
    let mut endpoints = Vec::new();
    let mut driven_fields = Vec::new();
    let mut consumed_fields = Vec::new();
    let mut clock_inputs = Vec::new();
    let mut reset_inputs = Vec::new();

    for param in &item.params {
        let endpoint = endpoint_for_local(
            tir,
            types,
            capabilities,
            protocols,
            param.id,
            &param.name,
            SummaryDirection::from(param.direction),
        );
        record_endpoint_effects(
            &endpoint,
            &mut driven_fields,
            &mut consumed_fields,
            &mut clock_inputs,
            &mut reset_inputs,
        );
        endpoints.push(endpoint);
    }
    if let Some(result) = &item.result {
        let endpoint = endpoint_for_result(tir, types, capabilities, protocols, result);
        record_endpoint_effects(
            &endpoint,
            &mut driven_fields,
            &mut consumed_fields,
            &mut clock_inputs,
            &mut reset_inputs,
        );
        endpoints.push(endpoint);
    }

    let domain_behavior = if clock_inputs.is_empty() && reset_inputs.is_empty() {
        SummaryDomainBehavior::Clockless
    } else {
        SummaryDomainBehavior::Explicit {
            clock_inputs,
            reset_inputs,
        }
    };
    let latency_class = match domain_behavior {
        SummaryDomainBehavior::Explicit { .. } => SummaryLatencyClass::Sequential,
        SummaryDomainBehavior::Clockless | SummaryDomainBehavior::Unknown => {
            SummaryLatencyClass::Unknown
        }
    };

    OpaqueItemSummary::builder(OpaqueItemKind::ExternModule, &item.name)
        .endpoints(endpoints)
        .driven_fields(driven_fields)
        .consumed_fields(consumed_fields)
        .domain_behavior(domain_behavior)
        .latency_class(latency_class)
        .protocol_preservation(SummaryProtocolPreservation::Unknown)
        .trust_boundary(super::TrustBoundary::SourceDerived)
        .build()
}

fn endpoint_for_result(
    tir: &TirDesign,
    types: &TypeTable,
    capabilities: &CapabilityTable,
    protocols: &ProtocolFacts,
    result: &HirSignatureResultBinding,
) -> SummaryEndpoint {
    endpoint_for_local(
        tir,
        types,
        capabilities,
        protocols,
        result.id,
        &result.name,
        SummaryDirection::Out,
    )
}

fn endpoint_for_local(
    tir: &TirDesign,
    types: &TypeTable,
    capabilities: &CapabilityTable,
    protocols: &ProtocolFacts,
    local: Option<syl_hir::LocalId>,
    name: &str,
    direction: SummaryDirection,
) -> SummaryEndpoint {
    let Some(local_id) = local else {
        return SummaryEndpoint::new(
            name,
            direction,
            SummaryLayout::Unknown,
            SummaryCapability::Unknown,
        );
    };
    let hir_id = HirFactId::Local(local_id);
    let layout = types
        .get(hir_id)
        .and_then(|type_id| tir.type_table().get(type_id))
        .map(|ty| summary_layout_for_type(tir, protocols, ty))
        .unwrap_or(SummaryLayout::Unknown);
    let capability = capabilities
        .get(hir_id)
        .map(|facts| OpaqueItemSummary::summary_capability_for_kind(tir, facts.kind()))
        .unwrap_or(SummaryCapability::Unknown);
    let endpoint = SummaryEndpoint::new(name, direction, layout, capability);
    types
        .get(hir_id)
        .and_then(|type_id| tir.type_table().get(type_id))
        .and_then(|ty| protocol_for_type(protocols, ty))
        .map_or(endpoint.clone(), |protocol| {
            endpoint.with_protocol(protocol)
        })
}

fn protocol_for_type(protocols: &ProtocolFacts, ty: &TirType) -> Option<SummaryProtocol> {
    match ty {
        TirType::View { base, .. } => base
            .definition()
            .and_then(|interface| protocols.get(interface))
            .map(SummaryProtocol::from),
        TirType::Named {
            def: Some(interface),
            kind: Some(HirDefKind::Interface),
            ..
        } => protocols.get(*interface).map(SummaryProtocol::from),
        _ => None,
    }
}

fn summary_layout_for_type(
    tir: &TirDesign,
    protocols: &ProtocolFacts,
    ty: &TirType,
) -> SummaryLayout {
    match ty {
        TirType::Unknown => SummaryLayout::Unknown,
        TirType::Nat => SummaryLayout::Nat,
        TirType::Bool => SummaryLayout::Bool,
        TirType::Bit => SummaryLayout::Bit,
        TirType::Clock { .. } => SummaryLayout::Clock,
        TirType::Reset { .. } => SummaryLayout::Reset,
        TirType::Domain => SummaryLayout::Domain,
        TirType::Str => SummaryLayout::Str,
        TirType::UInt { width } => SummaryLayout::Word {
            encoding: SummaryWordEncoding::UInt,
            width: super::SummaryLayoutConst::from(width),
        },
        TirType::Bits { width } => SummaryLayout::Word {
            encoding: SummaryWordEncoding::Bits,
            width: super::SummaryLayoutConst::from(width),
        },
        TirType::SInt { width } => SummaryLayout::Word {
            encoding: SummaryWordEncoding::SInt,
            width: super::SummaryLayoutConst::from(width),
        },
        TirType::Array { len, elem } => SummaryLayout::Array {
            len: super::SummaryLayoutConst::from(len),
            elem: Box::new(summary_layout_for_type(tir, protocols, elem)),
        },
        TirType::View { base, view } => {
            let protocol = base
                .definition()
                .and_then(|interface| protocols.get(interface))
                .map(SummaryProtocol::from);
            match protocol {
                Some(protocol) => SummaryLayout::View {
                    protocol: protocol.name().to_string(),
                    view: view.clone(),
                    fields: protocol.fields().to_vec(),
                },
                None => SummaryLayout::Opaque { label: ty.label() },
            }
        }
        TirType::Named {
            name,
            def: Some(def),
            kind: Some(HirDefKind::Bundle),
            ..
        } => SummaryLayout::Aggregate {
            name: name.clone(),
            fields: tir
                .hir()
                .bundles
                .get(def)
                .map(|bundle| {
                    bundle
                        .fields
                        .iter()
                        .map(|field| field.name.clone())
                        .collect()
                })
                .unwrap_or_default(),
        },
        TirType::Named {
            name,
            def: Some(def),
            kind: Some(HirDefKind::Interface),
            ..
        } => SummaryLayout::Aggregate {
            name: name.clone(),
            fields: protocols
                .get(*def)
                .map(|summary| summary.fields().to_vec())
                .unwrap_or_default(),
        },
        TirType::Named {
            name,
            def: Some(def),
            kind: Some(HirDefKind::Enum),
            ..
        } => SummaryLayout::Enum {
            name: name.clone(),
            variants: tir
                .hir()
                .enums
                .get(def)
                .map(|item| {
                    item.variants
                        .iter()
                        .map(|variant| variant.name.clone())
                        .collect()
                })
                .unwrap_or_default(),
        },
        TirType::Named { name, .. } => SummaryLayout::Opaque {
            label: name.clone(),
        },
    }
}

fn record_endpoint_effects(
    endpoint: &SummaryEndpoint,
    driven_fields: &mut Vec<SummaryPath>,
    consumed_fields: &mut Vec<SummaryPath>,
    clock_inputs: &mut Vec<String>,
    reset_inputs: &mut Vec<String>,
) {
    match endpoint.capability() {
        SummaryCapability::Clock { .. } => clock_inputs.push(endpoint.name().to_string()),
        SummaryCapability::Reset { .. } => reset_inputs.push(endpoint.name().to_string()),
        SummaryCapability::View {
            readable_fields,
            writable_fields,
            ..
        } => {
            for field in readable_fields {
                push_path(
                    consumed_fields,
                    SummaryPath::new(endpoint.name()).with_field(field),
                );
            }
            for field in writable_fields {
                push_path(
                    driven_fields,
                    SummaryPath::new(endpoint.name()).with_field(field),
                );
            }
        }
        SummaryCapability::Unknown | SummaryCapability::Value | SummaryCapability::Domain => {
            match endpoint.direction() {
                SummaryDirection::In => {
                    push_path(consumed_fields, SummaryPath::new(endpoint.name()))
                }
                SummaryDirection::Out => {
                    push_path(driven_fields, SummaryPath::new(endpoint.name()))
                }
            }
        }
    }
}

fn push_path(paths: &mut Vec<SummaryPath>, path: SummaryPath) {
    if paths.iter().any(|existing| existing == &path) {
        return;
    }
    paths.push(path);
}
