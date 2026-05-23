use std::borrow::Cow;
use syl_query::{
    CompletionItemKind, DefinitionResult, DocumentSymbolKind, DocumentSymbolResult, HoverResult,
};
use syl_session::ProjectError;
use syl_span::{SourcePosition, SourceRange};
use tower_lsp::{
    jsonrpc::Error as LspError,
    lsp_types::{
        DocumentSymbol, Hover, HoverContents, Location, MarkedString, Position, Range, SymbolKind,
        Url,
    },
};

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct LspMapper {
    position_limit: u32,
}

impl LspMapper {
    pub(crate) fn new() -> Self {
        Self {
            position_limit: u32::MAX,
        }
    }

    pub(crate) fn range(&self, range: SourceRange) -> Range {
        Range {
            start: self.position(range.start),
            end: self.position(range.end),
        }
    }

    pub(crate) fn position(&self, position: SourcePosition) -> Position {
        Position {
            line: self.bounded_u32(position.line),
            character: self.bounded_u32(position.character),
        }
    }

    pub(crate) fn source_position(&self, position: Position) -> SourcePosition {
        SourcePosition::new(
            usize::try_from(position.line).unwrap_or(usize::MAX),
            usize::try_from(position.character).unwrap_or(usize::MAX),
        )
    }

    pub(crate) fn project_error(&self, error: ProjectError) -> LspError {
        let mut lsp_error = LspError::internal_error();
        lsp_error.message = Cow::Owned(error.to_string());
        lsp_error
    }

    pub(crate) fn completion_kind(
        &self,
        kind: CompletionItemKind,
    ) -> Option<tower_lsp::lsp_types::CompletionItemKind> {
        match kind {
            CompletionItemKind::Module => Some(tower_lsp::lsp_types::CompletionItemKind::MODULE),
            CompletionItemKind::Function => {
                Some(tower_lsp::lsp_types::CompletionItemKind::FUNCTION)
            }
            CompletionItemKind::Type => Some(tower_lsp::lsp_types::CompletionItemKind::STRUCT),
            CompletionItemKind::Constant => {
                Some(tower_lsp::lsp_types::CompletionItemKind::CONSTANT)
            }
            CompletionItemKind::Field => Some(tower_lsp::lsp_types::CompletionItemKind::FIELD),
            CompletionItemKind::Keyword => Some(tower_lsp::lsp_types::CompletionItemKind::KEYWORD),
            _ => None,
        }
    }

    pub(crate) fn definition_location(&self, definition: DefinitionResult) -> Option<Location> {
        let uri = Url::parse(definition.uri.as_str()).ok()?;
        Some(Location::new(uri, self.range(definition.range)))
    }

    pub(crate) fn hover(&self, hover: HoverResult) -> Hover {
        Hover {
            contents: HoverContents::Scalar(MarkedString::String(hover.contents)),
            range: hover.range.map(|range| self.range(range)),
        }
    }

    #[allow(deprecated)]
    pub(crate) fn document_symbol(&self, symbol: DocumentSymbolResult) -> DocumentSymbol {
        let children = if symbol.children.is_empty() {
            None
        } else {
            Some(
                symbol
                    .children
                    .into_iter()
                    .map(|child| self.document_symbol(child))
                    .collect(),
            )
        };
        DocumentSymbol {
            name: symbol.name,
            detail: None,
            kind: self.document_symbol_kind(symbol.kind),
            tags: None,
            deprecated: None,
            range: self.range(symbol.range),
            selection_range: self.range(symbol.selection_range),
            children,
        }
    }

    fn document_symbol_kind(&self, kind: DocumentSymbolKind) -> SymbolKind {
        match kind {
            DocumentSymbolKind::Package => SymbolKind::PACKAGE,
            DocumentSymbolKind::Module => SymbolKind::MODULE,
            DocumentSymbolKind::Function => SymbolKind::FUNCTION,
            DocumentSymbolKind::Type => SymbolKind::STRUCT,
            DocumentSymbolKind::Constant => SymbolKind::CONSTANT,
            DocumentSymbolKind::Field => SymbolKind::FIELD,
            DocumentSymbolKind::Variable => SymbolKind::VARIABLE,
            DocumentSymbolKind::Parameter => SymbolKind::VARIABLE,
            DocumentSymbolKind::EnumMember => SymbolKind::ENUM_MEMBER,
            DocumentSymbolKind::View => SymbolKind::NAMESPACE,
            _ => SymbolKind::OBJECT,
        }
    }

    fn bounded_u32(&self, value: usize) -> u32 {
        u32::try_from(value).unwrap_or(self.position_limit)
    }
}

impl Default for LspMapper {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_mapping_preserves_contents_and_optional_range() {
        let mapper = LspMapper::new();
        let hover = HoverResult::new(
            "generic T",
            Some(SourceRange::new(
                SourcePosition::new(3, 10),
                SourcePosition::new(3, 11),
            )),
        );

        let mapped = mapper.hover(hover);

        assert!(matches!(
            mapped.contents,
            HoverContents::Scalar(MarkedString::String(ref contents)) if contents == "generic T"
        ));
        let range = mapped.range.expect("hover range should be mapped");
        assert_eq!(range.start.line, 3);
        assert_eq!(range.start.character, 10);
    }
}
