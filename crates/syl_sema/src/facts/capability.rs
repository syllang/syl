use super::{HirFactId, ProtocolFacts, ProtocolFieldDirection, TypeTable};
use crate::{
    TypeId,
    hir::HirLocalKind,
    tir::{TirDesign, TirType},
};
use std::collections::BTreeMap;
use syl_hir::DefId;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ViewCapabilityFacts {
    interface: DefId,
    view: String,
    readable_fields: Vec<String>,
    writable_fields: Vec<String>,
}

impl ViewCapabilityFacts {
    fn new(
        interface: DefId,
        view: String,
        readable_fields: Vec<String>,
        writable_fields: Vec<String>,
    ) -> Self {
        Self {
            interface,
            view,
            readable_fields,
            writable_fields,
        }
    }

    pub fn interface(&self) -> DefId {
        self.interface
    }

    pub fn view(&self) -> &str {
        &self.view
    }

    pub fn readable_fields(&self) -> &[String] {
        &self.readable_fields
    }

    pub fn writable_fields(&self) -> &[String] {
        &self.writable_fields
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DomainFact {
    Named(TypeId),
    BuiltinDomain,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CapabilityKind {
    Value,
    Domain,
    Clock { domain: DomainFact },
    Reset { domain: DomainFact },
    View(ViewCapabilityFacts),
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilityFacts {
    type_id: TypeId,
    kind: CapabilityKind,
}

impl CapabilityFacts {
    fn new(type_id: TypeId, kind: CapabilityKind) -> Self {
        Self { type_id, kind }
    }

    pub fn type_id(&self) -> TypeId {
        self.type_id
    }

    pub fn kind(&self) -> &CapabilityKind {
        &self.kind
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CapabilityTable {
    values: BTreeMap<HirFactId, CapabilityFacts>,
}

impl CapabilityTable {
    pub(crate) fn collect(tir: &TirDesign, types: &TypeTable, protocols: &ProtocolFacts) -> Self {
        let mut values = BTreeMap::new();
        let context = CapabilityKindContext::new(tir, protocols, types);
        for (id, ty_id) in types.raw_values() {
            let Some(kind) = capability_kind_for_id(&context, *id, *ty_id) else {
                continue;
            };
            values.insert(*id, CapabilityFacts::new(*ty_id, kind));
        }
        Self { values }
    }

    pub fn get(&self, id: HirFactId) -> Option<&CapabilityFacts> {
        self.values.get(&id)
    }
}

struct CapabilityKindContext<'a> {
    tir: &'a TirDesign,
    protocols: &'a ProtocolFacts,
    types: &'a TypeTable,
}

impl<'a> CapabilityKindContext<'a> {
    fn new(tir: &'a TirDesign, protocols: &'a ProtocolFacts, types: &'a TypeTable) -> Self {
        Self {
            tir,
            protocols,
            types,
        }
    }
}

fn capability_kind_for_id(
    context: &CapabilityKindContext<'_>,
    id: HirFactId,
    ty_id: TypeId,
) -> Option<CapabilityKind> {
    let ty = context.types.type_table().get(ty_id)?;
    match ty {
        TirType::Domain => Some(CapabilityKind::Domain),
        TirType::Clock { domain } => Some(CapabilityKind::Clock {
            domain: domain
                .as_deref()
                .map(|value| domain_fact_for_type(context.types, value))
                .unwrap_or(DomainFact::Unknown),
        }),
        TirType::Reset { domain } => Some(CapabilityKind::Reset {
            domain: domain
                .as_deref()
                .map(|value| domain_fact_for_type(context.types, value))
                .unwrap_or(DomainFact::Unknown),
        }),
        TirType::View { base, view } => {
            let HirFactId::Local(local_id) = id else {
                return None;
            };
            let interface = base.definition()?;
            let summary = context.protocols.get(interface)?;
            let view_summary = summary
                .views()
                .iter()
                .find(|candidate| candidate.name() == view)?;
            let local = context.tir.hir().locals.get(local_id.get())?;
            let mut readable = Vec::new();
            let mut writable = Vec::new();
            for field in view_summary.fields() {
                match (local.kind, *field.direction()) {
                    (HirLocalKind::Signal, ProtocolFieldDirection::In) => {
                        readable.push(field.name().to_string())
                    }
                    (_, ProtocolFieldDirection::InOut) => {
                        readable.push(field.name().to_string());
                        writable.push(field.name().to_string());
                    }
                    (HirLocalKind::Signal, ProtocolFieldDirection::Out) => {
                        writable.push(field.name().to_string())
                    }
                    (_, ProtocolFieldDirection::In) => readable.push(field.name().to_string()),
                    (_, ProtocolFieldDirection::Out) => writable.push(field.name().to_string()),
                }
            }
            Some(CapabilityKind::View(ViewCapabilityFacts::new(
                interface,
                view.clone(),
                readable,
                writable,
            )))
        }
        _ => Some(CapabilityKind::Value),
    }
}

fn domain_fact_for_type(types: &TypeTable, target: &TirType) -> DomainFact {
    match target {
        TirType::Domain => DomainFact::BuiltinDomain,
        TirType::Named {
            generic: Some(local),
            ..
        } => types
            .get(HirFactId::Local(*local))
            .map(DomainFact::Named)
            .unwrap_or(DomainFact::Unknown),
        _ => DomainFact::Unknown,
    }
}
