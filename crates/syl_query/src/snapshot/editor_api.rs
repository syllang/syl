use super::{inlay_hints, references, rename, semantic_tokens, signature_help, workspace_symbols};
use crate::*;
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri, ProjectError};
use syl_span::{SourcePosition, SourceRange};

/// Cancellable editor queries over immutable compiler facts. Positions use UTF-16.
pub trait EditorQueries {
    fn references_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        include_declaration: bool,
    ) -> Vec<LocationResult>;

    fn references_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        include_declaration: bool,
        token: &CancellationToken,
    ) -> Result<Vec<LocationResult>, QueryError>;

    fn prepare_rename_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> Option<PrepareRenameResult>;

    fn prepare_rename_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<PrepareRenameResult>, QueryError>;

    fn rename_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        new_name: &str,
    ) -> Result<Option<WorkspaceEditResult>, QueryError>;

    fn rename_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        new_name: &str,
        token: &CancellationToken,
    ) -> Result<Option<WorkspaceEditResult>, QueryError>;

    fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbolResult>;

    fn workspace_symbols_with_token(
        &self,
        query: &str,
        token: &CancellationToken,
    ) -> Result<Vec<WorkspaceSymbolResult>, QueryError>;

    fn inlay_hints(&self, uri: &DocumentUri, range: SourceRange) -> Vec<InlayHintResult>;

    fn inlay_hints_with_token(
        &self,
        uri: &DocumentUri,
        range: SourceRange,
        token: &CancellationToken,
    ) -> Result<Vec<InlayHintResult>, QueryError>;

    fn semantic_tokens(&self, uri: &DocumentUri) -> Vec<SemanticTokenResult>;

    fn semantic_tokens_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Vec<SemanticTokenResult>, QueryError>;

    fn signature_help_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> Option<SignatureHelpResult>;

    fn signature_help_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<SignatureHelpResult>, QueryError>;
}

impl EditorQueries for AnalysisSnapshot {
    fn references_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        include_declaration: bool,
    ) -> Vec<LocationResult> {
        self.references_at_with_token(
            uri,
            utf16_position,
            include_declaration,
            &CancellationToken::new(),
        )
        .unwrap_or_default()
    }
    fn references_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        include_declaration: bool,
        token: &CancellationToken,
    ) -> Result<Vec<LocationResult>, QueryError> {
        references::query(self, uri, utf16_position, include_declaration, token)
    }
    fn prepare_rename_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> Option<PrepareRenameResult> {
        self.prepare_rename_at_with_token(uri, utf16_position, &CancellationToken::new())
            .unwrap_or_default()
    }
    fn prepare_rename_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<PrepareRenameResult>, QueryError> {
        rename::prepare(self, uri, utf16_position, token)
    }
    fn rename_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        new_name: &str,
    ) -> Result<Option<WorkspaceEditResult>, QueryError> {
        self.rename_at_with_token(uri, utf16_position, new_name, &CancellationToken::new())
    }
    fn rename_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        new_name: &str,
        token: &CancellationToken,
    ) -> Result<Option<WorkspaceEditResult>, QueryError> {
        rename::rename(self, uri, utf16_position, new_name, token)
    }
    fn workspace_symbols(&self, query: &str) -> Vec<WorkspaceSymbolResult> {
        self.workspace_symbols_with_token(query, &CancellationToken::new())
            .unwrap_or_default()
    }
    fn workspace_symbols_with_token(
        &self,
        query: &str,
        token: &CancellationToken,
    ) -> Result<Vec<WorkspaceSymbolResult>, QueryError> {
        workspace_symbols::query(self, query, token)
    }
    fn inlay_hints(&self, uri: &DocumentUri, range: SourceRange) -> Vec<InlayHintResult> {
        self.inlay_hints_with_token(uri, range, &CancellationToken::new())
            .unwrap_or_default()
    }
    fn inlay_hints_with_token(
        &self,
        uri: &DocumentUri,
        range: SourceRange,
        token: &CancellationToken,
    ) -> Result<Vec<InlayHintResult>, QueryError> {
        inlay_hints::query(self, uri, range, token)
    }
    fn semantic_tokens(&self, uri: &DocumentUri) -> Vec<SemanticTokenResult> {
        self.semantic_tokens_with_token(uri, &CancellationToken::new())
            .unwrap_or_default()
    }
    fn semantic_tokens_with_token(
        &self,
        uri: &DocumentUri,
        token: &CancellationToken,
    ) -> Result<Vec<SemanticTokenResult>, QueryError> {
        semantic_tokens::query(self, uri, token)
    }
    fn signature_help_at(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
    ) -> Option<SignatureHelpResult> {
        self.signature_help_at_with_token(uri, utf16_position, &CancellationToken::new())
            .unwrap_or_default()
    }
    fn signature_help_at_with_token(
        &self,
        uri: &DocumentUri,
        utf16_position: SourcePosition,
        token: &CancellationToken,
    ) -> Result<Option<SignatureHelpResult>, QueryError> {
        signature_help::query(self, uri, utf16_position, token)
    }
}

pub(super) fn check(token: &CancellationToken) -> Result<(), QueryError> {
    if token.is_cancelled() {
        Err(QueryError::Cancelled)
    } else {
        Ok(())
    }
}

pub(super) fn map_error(error: ProjectError) -> QueryError {
    match error {
        ProjectError::Cancelled => QueryError::Cancelled,
        other => unreachable!("snapshot query failed: {other}"),
    }
}
