//! Stable user-facing facade for Syl.
//!
//! This crate is intentionally thin: it re-exports the stable entry points that
//! embedding applications normally need for parsing, diagnostics, project
//! sessions, editor-neutral queries, and SystemVerilog emission.
//!
//! Compiler stages and hardware internals remain owned by their specific
//! crates. Use those crates directly only when working on compiler internals or
//! stage-specific tooling.

pub use syl_emit::SystemVerilogBackend;
pub use syl_query::{AnalysisQueries, ProjectQueries};
pub use syl_session::{
    AnalysisFile, AnalysisHost, AnalysisSnapshot, DocumentOrigin, DocumentUri, DocumentVersion,
    FsVfs, Project, ProjectConfig, ProjectConfigBuilder, ProjectError, ProjectResolver,
    SourceDocument, Vfs,
};
pub use syl_span::{
    Diagnostic, DiagnosticRelatedInfo, DiagnosticSeverity, SourceFile, SourceId, SourceMap,
    SourcePosition, SourceRange, Span,
};
pub use syl_syntax::{AstFile, ParseOutput, SourceParser};
