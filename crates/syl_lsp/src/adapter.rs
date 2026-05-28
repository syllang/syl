use crate::{
    diagnostics::{LspDiagnosticPublication, LspDiagnostics},
    mapping::LspMapper,
};
use syl_query::{
    CompletionResult, DefinitionResult, DocumentSymbolResult, GroupedDiagnostics, HoverResult,
    QueryError,
};
use syl_session::ProjectError;
use syl_span::SourcePosition;
use tower_lsp::{
    jsonrpc::{Error as LspError, ErrorCode},
    lsp_types::{
        CompletionResponse, DocumentSymbolResponse, GotoDefinitionResponse, Hover, Position,
    },
};

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct LspAdapter {
    mapper: LspMapper,
}

impl LspAdapter {
    pub(crate) fn new() -> Self {
        Self {
            mapper: LspMapper::new(),
        }
    }

    pub(crate) fn source_position(&self, position: Position) -> SourcePosition {
        self.mapper.source_position(position)
    }

    pub(crate) fn project_error(&self, error: ProjectError) -> LspError {
        self.mapper.project_error(error)
    }

    pub(crate) fn query_error(&self, error: QueryError) -> LspError {
        match error {
            QueryError::Cancelled => LspError {
                code: ErrorCode::RequestCancelled,
                message: error.to_string().into(),
                data: None,
            },
            _ => LspError {
                code: ErrorCode::InternalError,
                message: "unsupported query error".into(),
                data: None,
            },
        }
    }

    pub(crate) fn hover(&self, hover: HoverResult) -> Hover {
        self.mapper.hover(hover)
    }

    pub(crate) fn definition(
        &self,
        definition: DefinitionResult,
    ) -> Option<GotoDefinitionResponse> {
        self.mapper
            .definition_location(definition)
            .map(GotoDefinitionResponse::Scalar)
    }

    pub(crate) fn completion(&self, completion: CompletionResult) -> CompletionResponse {
        CompletionResponse::Array(
            completion
                .items
                .into_iter()
                .map(|item| tower_lsp::lsp_types::CompletionItem {
                    kind: self.mapper.completion_kind(item.kind),
                    label: item.label,
                    ..tower_lsp::lsp_types::CompletionItem::default()
                })
                .collect(),
        )
    }

    pub(crate) fn document_symbols(
        &self,
        symbols: Vec<DocumentSymbolResult>,
    ) -> DocumentSymbolResponse {
        DocumentSymbolResponse::Nested(
            symbols
                .into_iter()
                .map(|symbol| self.mapper.document_symbol(symbol))
                .collect(),
        )
    }

    pub(crate) fn diagnostic_publications(
        &self,
        grouped: &GroupedDiagnostics,
    ) -> Vec<LspDiagnosticPublication> {
        LspDiagnostics::new(grouped).publications()
    }
}
