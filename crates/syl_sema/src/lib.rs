mod ids;
pub use ids::TypeId;

pub mod actual_binding;
pub mod analysis;
mod capability;
mod capability_model;
pub mod cell_summary;
pub mod completion;
pub mod const_eval;
pub mod const_mir;
pub mod diagnostic;
pub mod error;
pub mod facts;
mod hir;
mod hir_lower;
mod hir_resolve;
mod hir_view;
pub mod map_ir;
pub mod mir;
mod mir_type_resolve;
pub mod opaque_summary;
mod stage_output;
pub mod tir;
mod tir_const;

pub use analysis::{
    DefinitionInfo, HirAnalysis, HirAnalysisOutput, HoverInfo, SemanticCompiler, SemanticOutput,
    SemanticSession, TirAnalysis,
};
pub use diagnostic::{SemanticDiagnostic, SemanticDiagnosticStage};
pub use error::{
    CapabilityError, CompileError, ConstEvalError, DriverError, EirError, HirError, HwirError,
    LoweringError, TirError,
};
pub use facts::{
    CapabilityFacts, CapabilityKind, CapabilityTable, ConstFactKey, ConstFacts, DefinitionKind,
    DefinitionPath, DomainFact, HirFactId, ImportEdge, ImportId, Layout, LayoutConst, LayoutFacts,
    PackageNodeId, PackageSummary, ProtocolFacts, ProtocolFieldDirection, ProtocolSummary,
    ProtocolViewSummary, ResolutionGraph, ResolutionTable, SemanticFacts, SemanticResolution,
    TypeTable, ViewCapabilityFacts, ViewFieldSummary, WordEncoding,
};
pub use hir_lower::HirResolver;
pub use opaque_summary::{
    BackendConstraint, OpaqueItemKind, OpaqueItemSummary, OpaqueItemSummaryBuilder,
    OpaqueSummaryTable, SummaryCapability, SummaryDirection, SummaryDomain, SummaryDomainBehavior,
    SummaryEndpoint, SummaryFieldDirection, SummaryLatencyClass, SummaryLayout, SummaryLayoutConst,
    SummaryPath, SummaryProtocol, SummaryProtocolPreservation, SummaryView, SummaryViewField,
    SummaryWordEncoding, TrustBoundary,
};
pub use stage_output::StageOutput;
