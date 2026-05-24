mod error;
mod navigation;
mod snapshot;

#[cfg(test)]
mod tests;

pub use error::QueryError;
pub use navigation::{
    CompletionItem, CompletionItemKind, CompletionResult, DefinitionResult, DiagnosticPackage,
    DiagnosticRelatedResult, DiagnosticResult, DiagnosticStage, DocumentDiagnostics,
    DocumentSymbolKind, DocumentSymbolResult, GroupedDiagnostics, HoverResult, PackageDiagnostics,
    StageDiagnostics,
};
pub use snapshot::{AnalysisQueries, ProjectQueries};
