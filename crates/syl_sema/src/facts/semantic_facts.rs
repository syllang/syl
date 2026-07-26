use super::{CapabilityTable, ConstFacts, LayoutFacts, ProtocolFacts, ResolutionTable, TypeTable};
use crate::summary::opaque::OpaqueSummaryTable;
use crate::tir::TirDesign;
use getset::Getters;

/// Collected semantic side tables over a type-checked design.
///
/// Aggregate root for facts: fields are private; use derived getters.
/// Constructed from a finished [`TirDesign`] via crate-internal collection.
#[derive(Clone, Debug, PartialEq, Eq, Getters)]
#[getset(get = "pub")]
#[non_exhaustive]
pub struct SemanticFacts {
    resolution: ResolutionTable,
    types: TypeTable,
    capabilities: CapabilityTable,
    consts: ConstFacts,
    layouts: LayoutFacts,
    protocols: ProtocolFacts,
    opaque_summaries: OpaqueSummaryTable,
}

impl SemanticFacts {
    pub(crate) fn collect(tir: &TirDesign) -> Self {
        let resolution = ResolutionTable::collect(tir.hir());
        let types = TypeTable::collect(tir);
        let protocols = ProtocolFacts::collect(tir.hir());
        let capabilities = CapabilityTable::collect(tir, &types, &protocols);
        let consts = ConstFacts::collect(tir);
        let layouts = LayoutFacts::collect(tir, &protocols);
        let opaque_summaries = OpaqueSummaryTable::collect(tir, &types, &capabilities, &protocols);
        Self {
            resolution,
            types,
            capabilities,
            consts,
            layouts,
            protocols,
            opaque_summaries,
        }
    }
}
