mod capability;
mod consts;
mod layout;
mod protocol;
mod resolution;
mod types;

use crate::tir::TirDesign;

pub use capability::{
    CapabilityFacts, CapabilityKind, CapabilityTable, DomainFact, ViewCapabilityFacts,
};
pub use consts::{ConstFactKey, ConstFacts};
pub use layout::{Layout, LayoutConst, LayoutFacts, WordEncoding};
pub use protocol::{
    ProtocolFacts, ProtocolFieldDirection, ProtocolSummary, ProtocolViewSummary, ViewFieldSummary,
};
pub use resolution::{
    DefinitionKind, DefinitionPath, HirFactId, ImportEdge, ImportId, PackageNodeId, PackageSummary,
    ResolutionGraph, ResolutionTable, SemanticResolution,
};
pub use types::TypeTable;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SemanticFacts {
    resolution: ResolutionTable,
    types: TypeTable,
    capabilities: CapabilityTable,
    consts: ConstFacts,
    layouts: LayoutFacts,
    protocols: ProtocolFacts,
}

impl SemanticFacts {
    pub(crate) fn empty() -> Self {
        Self {
            resolution: ResolutionTable::empty(),
            types: TypeTable::empty(),
            capabilities: CapabilityTable::empty(),
            consts: ConstFacts::empty(),
            layouts: LayoutFacts::empty(),
            protocols: ProtocolFacts::empty(),
        }
    }

    pub(crate) fn collect(tir: &TirDesign) -> Self {
        let resolution = ResolutionTable::collect(tir.hir());
        let types = TypeTable::collect(tir);
        let protocols = ProtocolFacts::collect(tir.hir());
        let capabilities = CapabilityTable::collect(tir, &types, &protocols);
        let consts = ConstFacts::collect(tir);
        let layouts = LayoutFacts::collect(tir, &protocols);
        Self {
            resolution,
            types,
            capabilities,
            consts,
            layouts,
            protocols,
        }
    }

    pub fn resolution(&self) -> &ResolutionTable {
        &self.resolution
    }

    pub fn types(&self) -> &TypeTable {
        &self.types
    }

    pub fn capabilities(&self) -> &CapabilityTable {
        &self.capabilities
    }

    pub fn consts(&self) -> &ConstFacts {
        &self.consts
    }

    pub fn layouts(&self) -> &LayoutFacts {
        &self.layouts
    }

    pub fn protocols(&self) -> &ProtocolFacts {
        &self.protocols
    }
}
