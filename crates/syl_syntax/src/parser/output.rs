use crate::AstFile;
use syl_span::Diagnostic;

#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ParseOutput {
    pub file: AstFile,
    pub diagnostics: Vec<Diagnostic>,
    node_index: crate::AstNodeIndex,
}

impl ParseOutput {
    pub fn new(file: AstFile, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            file,
            diagnostics,
            node_index: crate::AstNodeIndex::default(),
        }
    }

    pub fn into_result(self) -> Result<AstFile, Vec<Diagnostic>> {
        if self.diagnostics.is_empty() {
            Ok(self.file)
        } else {
            Err(self.diagnostics)
        }
    }

    pub fn node_index(&self) -> &crate::AstNodeIndex {
        &self.node_index
    }

    pub(crate) fn attach_node_index(&mut self, source: &str) {
        self.node_index = self.file.build_node_index(source);
    }
}
