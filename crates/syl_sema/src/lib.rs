pub mod binding;
mod capability;
pub mod diagnostic;
pub mod facts;
mod hir;
pub mod ir;
pub mod pipeline;
pub mod query_support;
pub mod summary;
pub mod tir;

pub use tir::TypeId;

pub use binding::{ActualFormalBinder, FormalBinding};
pub use diagnostic::{
    CapabilityError, CompileError, ConstEvalError, DriverError, EirError, HirError, HwirError,
    LoweringError, SemanticDiagnostic, SemanticDiagnosticStage, TirError,
};
pub use facts::{
    CapabilityFacts, CapabilityKind, CapabilityTable, ConstFactKey, ConstFacts, DefinitionKind,
    DefinitionPath, DomainFact, HirFactId, ImportEdge, ImportId, Layout, LayoutConst, LayoutFacts,
    PackageNodeId, PackageSummary, ProtocolFacts, ProtocolFieldDirection, ProtocolSummary,
    ProtocolViewSummary, ResolutionGraph, ResolutionTable, SemanticFacts, SemanticResolution,
    TypeTable, ViewCapabilityFacts, ViewFieldSummary, WordEncoding,
};
pub use hir::lower::HirResolver;
pub use pipeline::{
    DefinitionInfo, HirAnalysis, HirAnalysisOutput, HoverInfo, SemanticCompiler, SemanticOutput,
    SemanticSession, SemanticSourceFile, StageOutput, TirAnalysis,
};
pub use query_support::{CompletionItem, CompletionKind};
pub use summary::{
    cell::{
        CellBoundarySummary, CellInstanceMatch, CellSummary, CellSummaryDeclaration,
        CellSummaryRegistry, CellSummaryStatus, HwOrigin, HwPlace, OpaqueCellSummary,
    },
    opaque::{
        BackendConstraint, OpaqueItemKind, OpaqueItemSummary, OpaqueItemSummaryBuilder,
        OpaqueSummaryTable, SummaryCapability, SummaryDirection, SummaryDomain,
        SummaryDomainBehavior, SummaryEndpoint, SummaryFieldDirection, SummaryLatencyClass,
        SummaryLayout, SummaryLayoutConst, SummaryPath, SummaryProtocol,
        SummaryProtocolPreservation, SummaryView, SummaryViewField, SummaryWordEncoding,
        TrustBoundary,
    },
};
