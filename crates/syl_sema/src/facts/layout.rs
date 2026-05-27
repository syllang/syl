use super::ProtocolFacts;
use crate::{
    TypeId,
    tir::{TirConstTerm, TirDesign, TirType},
};
use std::collections::BTreeMap;
use syl_hir::DefId;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LayoutConst {
    Known(u64),
    Symbol(String),
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum WordEncoding {
    UInt,
    Bits,
    SInt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Layout {
    Unknown,
    Nat,
    Bit,
    Bool,
    Domain,
    Clock,
    Reset,
    Str,
    Word {
        encoding: WordEncoding,
        width: LayoutConst,
    },
    Array {
        len: LayoutConst,
        elem: Box<Layout>,
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
        interface: DefId,
        view: String,
        fields: Vec<String>,
    },
    Opaque {
        label: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LayoutFacts {
    values: BTreeMap<TypeId, Layout>,
}

impl LayoutFacts {
    pub(crate) fn collect(tir: &TirDesign, protocols: &ProtocolFacts) -> Self {
        let values = tir
            .type_table()
            .iter()
            .map(|(id, ty)| (id, layout_for_type(tir, protocols, ty)))
            .collect();
        Self { values }
    }

    pub fn get(&self, type_id: TypeId) -> Option<&Layout> {
        self.values.get(&type_id)
    }
}

fn layout_for_type(tir: &TirDesign, protocols: &ProtocolFacts, ty: &TirType) -> Layout {
    match ty {
        TirType::Unknown => Layout::Unknown,
        TirType::Nat => Layout::Nat,
        TirType::Bool => Layout::Bool,
        TirType::Bit => Layout::Bit,
        TirType::Clock { .. } => Layout::Clock,
        TirType::Reset { .. } => Layout::Reset,
        TirType::Domain => Layout::Domain,
        TirType::Str => Layout::Str,
        TirType::UInt { width } => Layout::Word {
            encoding: WordEncoding::UInt,
            width: layout_const(width),
        },
        TirType::Bits { width } => Layout::Word {
            encoding: WordEncoding::Bits,
            width: layout_const(width),
        },
        TirType::SInt { width } => Layout::Word {
            encoding: WordEncoding::SInt,
            width: layout_const(width),
        },
        TirType::Array { len, elem } => Layout::Array {
            len: layout_const(len),
            elem: Box::new(layout_for_type(tir, protocols, elem)),
        },
        TirType::View { base, view } => {
            let interface = base.definition();
            if let Some(interface) = interface
                && let Some(summary) = protocols.get(interface)
                && let Some(view_summary) = summary
                    .views()
                    .iter()
                    .find(|candidate| candidate.name() == view)
            {
                return Layout::View {
                    interface,
                    view: view.clone(),
                    fields: view_summary
                        .fields()
                        .iter()
                        .map(|field| field.name().to_string())
                        .collect(),
                };
            }
            Layout::Opaque { label: ty.label() }
        }
        TirType::Named {
            name, def, kind, ..
        } => match (*def, *kind) {
            (Some(def), Some(crate::hir::HirDefKind::Bundle)) => Layout::Aggregate {
                name: name.clone(),
                fields: tir
                    .hir()
                    .bundles
                    .get(&def)
                    .map(|bundle| {
                        bundle
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            (Some(def), Some(crate::hir::HirDefKind::Interface)) => Layout::Aggregate {
                name: name.clone(),
                fields: protocols
                    .get(def)
                    .map(|summary| summary.fields().to_vec())
                    .unwrap_or_default(),
            },
            (Some(def), Some(crate::hir::HirDefKind::Enum)) => Layout::Enum {
                name: name.clone(),
                variants: tir
                    .hir()
                    .enums
                    .get(&def)
                    .map(|item| {
                        item.variants
                            .iter()
                            .map(|variant| variant.name.clone())
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            _ => Layout::Opaque {
                label: name.clone(),
            },
        },
    }
}

fn layout_const(term: &TirConstTerm) -> LayoutConst {
    match term {
        TirConstTerm::NatLiteral(value) => LayoutConst::Known(*value),
        TirConstTerm::Named { name, .. } => LayoutConst::Symbol(name.clone()),
        TirConstTerm::Expr { label } => LayoutConst::Symbol(label.clone()),
        TirConstTerm::Unknown | TirConstTerm::BoolLiteral(_) => LayoutConst::Unknown,
    }
}
