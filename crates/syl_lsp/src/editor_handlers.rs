use crate::{
    SylLanguageServer, cancellation::CancellationSlot, editor_adapter as adapter,
    mapping::LspMapper,
};
use syl_query::{EditorQueries, QueryError};
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri};
use syl_span::SourceRange;
use tower_lsp::{
    jsonrpc::{Error, Result},
    lsp_types::*,
};

/// Dropping a protocol request also stops its blocking query cooperatively.
struct RequestGuard(CancellationToken);
impl Drop for RequestGuard {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl SylLanguageServer {
    pub(super) async fn editor_query<T, F>(&self, slot: CancellationSlot, query: F) -> Result<T>
    where
        T: Send + 'static,
        F: FnOnce(&AnalysisSnapshot, CancellationToken) -> std::result::Result<T, QueryError>
            + Send
            + 'static,
    {
        let token = self.cancellation_registry.replace(slot);
        let _guard = RequestGuard(token.clone());
        let snapshot = self
            .analysis_snapshot_with_token(&token)
            .await
            .map_err(|e| self.adapter.project_error(e))?;
        let query_token = token.clone();
        let result = tokio::task::spawn_blocking(move || query(&snapshot, query_token))
            .await
            .map_err(|_| Error::internal_error())?;
        if token.is_cancelled() {
            return Err(adapter::query_error(QueryError::Cancelled));
        }
        result.map_err(adapter::query_error)
    }

    pub(super) async fn handle_references(
        &self,
        params: ReferenceParams,
    ) -> Result<Option<Vec<Location>>> {
        let uri = DocumentUri::new(params.text_document_position.text_document.uri.to_string());
        let position = self
            .adapter
            .source_position(params.text_document_position.position);
        let results = self
            .editor_query(CancellationSlot::References, move |s, t| {
                s.references_at_with_token(&uri, position, params.context.include_declaration, &t)
            })
            .await?;
        Ok(Some(
            results.into_iter().filter_map(adapter::location).collect(),
        ))
    }

    pub(super) async fn handle_prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = DocumentUri::new(params.text_document.uri.to_string());
        let position = self.adapter.source_position(params.position);
        let result = self
            .editor_query(CancellationSlot::Rename, move |s, t| {
                s.prepare_rename_at_with_token(&uri, position, &t)
            })
            .await?;
        Ok(result.map(adapter::prepare_rename))
    }

    pub(super) async fn handle_rename(
        &self,
        params: RenameParams,
    ) -> Result<Option<WorkspaceEdit>> {
        let uri = DocumentUri::new(params.text_document_position.text_document.uri.to_string());
        let position = self
            .adapter
            .source_position(params.text_document_position.position);
        let result = self
            .editor_query(CancellationSlot::Rename, move |s, t| {
                s.rename_at_with_token(&uri, position, &params.new_name, &t)
            })
            .await?;
        result.map(adapter::workspace_edit).transpose()
    }

    pub(super) async fn handle_symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let results = self
            .editor_query(CancellationSlot::WorkspaceSymbols, move |s, t| {
                s.workspace_symbols_with_token(&params.query, &t)
            })
            .await?;
        Ok(Some(
            results.into_iter().filter_map(adapter::symbol).collect(),
        ))
    }

    pub(super) async fn handle_inlay_hint(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = DocumentUri::new(params.text_document.uri.to_string());
        let mapper = LspMapper::new();
        let range = SourceRange::new(
            mapper.source_position(params.range.start),
            mapper.source_position(params.range.end),
        );
        let results = self
            .editor_query(CancellationSlot::InlayHints, move |s, t| {
                s.inlay_hints_with_token(&uri, range, &t)
            })
            .await?;
        Ok(Some(results.into_iter().map(adapter::inlay_hint).collect()))
    }

    pub(super) async fn handle_semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = DocumentUri::new(params.text_document.uri.to_string());
        let results = self
            .editor_query(CancellationSlot::SemanticTokens, move |s, t| {
                s.semantic_tokens_with_token(&uri, &t)
            })
            .await?;
        Ok(Some(adapter::encode_semantic_tokens(results)))
    }

    pub(super) async fn handle_signature_help(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        let uri = DocumentUri::new(
            params
                .text_document_position_params
                .text_document
                .uri
                .to_string(),
        );
        let position = self
            .adapter
            .source_position(params.text_document_position_params.position);
        let result = self
            .editor_query(CancellationSlot::SignatureHelp, move |s, t| {
                s.signature_help_at_with_token(&uri, position, &t)
            })
            .await?;
        Ok(result.map(adapter::signature_help))
    }
}

#[cfg(test)]
#[path = "editor_handlers_tests.rs"]
mod tests;
