use crate::AstFile;
use syl_span::Diagnostic;

/// The result of parsing a source file, preserving diagnostics alongside the AST.
///
/// **Error recovery:** Even when diagnostics contain errors, `file` contains a
/// best-effort AST built with error recovery — malformed constructs are replaced
/// with `ErrorItem` / `Stmt::Error` / `Expr::CompileError` nodes. This allows
/// IDE features (syntax highlighting, partial completion) to function even in
/// the presence of syntax errors.
///
/// **Warnings pass through:** Non-fatal diagnostics (warnings, hints) are also
/// collected here. Use `parse_file_partial` instead of `parse_file` when you
/// want to surface warnings without failing.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ParseOutput {
    pub file: AstFile,
    pub diagnostics: Vec<Diagnostic>,
    node_index: crate::AstNodeIndex,
}

impl ParseOutput {
    pub fn new(file: AstFile, diagnostics: Vec<Diagnostic>) -> Self {
        let node_index = file.build_node_index("");
        Self {
            file,
            diagnostics,
            node_index,
        }
    }

    pub(crate) fn with_node_index(
        file: AstFile,
        diagnostics: Vec<Diagnostic>,
        node_index: crate::AstNodeIndex,
    ) -> Self {
        Self {
            file,
            diagnostics,
            node_index,
        }
    }

    /// Converts this output into a `Result`, returning `Err` if any
    /// diagnostics are present, or the `AstFile` on success.
    pub fn into_result(self) -> Result<AstFile, Vec<Diagnostic>> {
        if self.diagnostics.is_empty() {
            Ok(self.file)
        } else {
            Err(self.diagnostics)
        }
    }

    /// Returns the node index that maps AST node kinds to their source locations.
    pub fn node_index(&self) -> &crate::AstNodeIndex {
        &self.node_index
    }
}
