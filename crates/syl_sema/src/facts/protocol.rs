use crate::hir::HirDesign;
use std::collections::BTreeMap;
use syl_hir::DefId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProtocolFieldDirection {
    In,
    InOut,
    Out,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ViewFieldSummary {
    name: String,
    direction: ProtocolFieldDirection,
}

impl ViewFieldSummary {
    fn new(name: String, direction: ProtocolFieldDirection) -> Self {
        Self { name, direction }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn direction(&self) -> &ProtocolFieldDirection {
        &self.direction
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProtocolViewSummary {
    name: String,
    fields: Vec<ViewFieldSummary>,
}

impl ProtocolViewSummary {
    fn new(name: String, fields: Vec<ViewFieldSummary>) -> Self {
        Self { name, fields }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[ViewFieldSummary] {
        &self.fields
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProtocolSummary {
    interface: DefId,
    name: String,
    fields: Vec<String>,
    views: Vec<ProtocolViewSummary>,
}

impl ProtocolSummary {
    fn new(
        interface: DefId,
        name: String,
        fields: Vec<String>,
        views: Vec<ProtocolViewSummary>,
    ) -> Self {
        Self {
            interface,
            name,
            fields,
            views,
        }
    }

    pub fn interface(&self) -> DefId {
        self.interface
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn fields(&self) -> &[String] {
        &self.fields
    }

    pub fn views(&self) -> &[ProtocolViewSummary] {
        &self.views
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProtocolFacts {
    values: BTreeMap<DefId, ProtocolSummary>,
}

impl ProtocolFacts {
    pub(crate) fn collect(hir: &HirDesign) -> Self {
        let values = hir
            .interfaces
            .iter()
            .map(|(def, interface)| {
                let views = interface
                    .views
                    .iter()
                    .map(|view| {
                        let fields = view
                            .fields
                            .iter()
                            .map(|field| {
                                let direction = match field.direction {
                                    crate::hir::HirViewDirection::In => ProtocolFieldDirection::In,
                                    crate::hir::HirViewDirection::InOut => {
                                        ProtocolFieldDirection::InOut
                                    }
                                    crate::hir::HirViewDirection::Out => {
                                        ProtocolFieldDirection::Out
                                    }
                                    _ => unreachable!(
                                        "unsupported interface view direction in protocol facts"
                                    ),
                                };
                                ViewFieldSummary::new(field.name.clone(), direction)
                            })
                            .collect();
                        ProtocolViewSummary::new(view.name.clone(), fields)
                    })
                    .collect();
                let fields = interface
                    .fields
                    .iter()
                    .map(|field| field.name.clone())
                    .collect();
                (
                    *def,
                    ProtocolSummary::new(*def, interface.name.clone(), fields, views),
                )
            })
            .collect();
        Self { values }
    }

    pub fn get(&self, interface: DefId) -> Option<&ProtocolSummary> {
        self.values.get(&interface)
    }
}
