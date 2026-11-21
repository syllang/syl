mod api;
mod completion_context;
mod diagnostics;
#[cfg(test)]
mod diagnostics_tests;
mod document_symbols;
mod editor_api;
mod generic;
mod ident_span;
mod import_completion;
mod inlay_hints;
mod references;
mod rename;
mod semantic_tokens;
mod signature_help;
mod workspace_symbols;

pub use api::{AnalysisQueries, ProjectQueries};

#[cfg(test)]
pub(crate) use diagnostics::DiagnosticQueryEngine;

pub use editor_api::EditorQueries;

#[cfg(test)]
mod editor_tests;
