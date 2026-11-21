mod adapter;
mod cancellation;
mod diagnostics;
mod drc;
mod editor_adapter;
mod editor_handlers;
mod mapping;
mod sync;
mod workspace;

use tower_lsp::lsp_types as editor_types;

use adapter::LspAdapter;
use cancellation::{CancellationRegistry, CancellationSlot};
use diagnostics::{LspDiagnosticPublication, LspDiagnosticState};
use drc::BackgroundDrcScheduler;
use std::{sync::Arc, time::Duration};
use syl_query::AnalysisQueries;
use syl_session::{
    AnalysisHost, AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectError,
};
use tokio::sync::Mutex;
use tower_lsp::{
    Client, LanguageServer, LspService, Server,
    jsonrpc::Result as LspResult,
    lsp_types::{
        CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
        DidChangeWatchedFilesParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DocumentSymbolParams, DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse,
        Hover, HoverParams, InitializeParams, InitializeResult, OneOf, ServerCapabilities,
        TextDocumentSyncCapability, TextDocumentSyncKind, Url,
    },
};
use workspace::WorkspaceState;

#[derive(Debug)]
#[non_exhaustive]
pub struct SylLanguageServer {
    client: Client,
    host: Arc<Mutex<AnalysisHost>>,
    diagnostics: Arc<Mutex<LspDiagnosticState>>,
    workspace: Arc<Mutex<WorkspaceState>>,
    initialization_error: Arc<Mutex<Option<ProjectError>>>,
    drc_scheduler: Arc<BackgroundDrcScheduler>,
    cancellation_registry: Arc<CancellationRegistry>,
    adapter: LspAdapter,
}

impl SylLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            host: Arc::new(Mutex::new(AnalysisHost::new())),
            diagnostics: Arc::new(Mutex::new(LspDiagnosticState::new())),
            workspace: Arc::new(Mutex::new(WorkspaceState::default())),
            initialization_error: Arc::new(Mutex::new(None)),
            drc_scheduler: Arc::new(BackgroundDrcScheduler::new()),
            cancellation_registry: Arc::new(CancellationRegistry::new()),
            adapter: LspAdapter::new(),
        }
    }

    async fn analysis_snapshot_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<Arc<AnalysisSnapshot>, ProjectError> {
        self.drc_scheduler
            .snapshot(
                Arc::clone(&self.host),
                self.drc_scheduler.generation(),
                token.clone(),
            )
            .await
    }

    fn next_diagnostics_generation(&self) -> u64 {
        self.drc_scheduler
            .next_generation(&self.cancellation_registry)
    }

    async fn publish_project_error(&self, uri: Url, error: ProjectError, generation: u64) {
        let publication = LspDiagnosticPublication::project_error(uri, error);
        let mut state = self.diagnostics.lock().await;
        if !self.drc_scheduler.is_current(generation) {
            return;
        }
        let publications = state.record_project_error(publication);
        for publication in publications {
            let (target_uri, diagnostics, version) = publication.into_parts();
            self.client
                .publish_diagnostics(target_uri, diagnostics, version)
                .await;
        }
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SylLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        let mut workspace = WorkspaceState::new(&params);
        let error = workspace.configure_host(&mut *self.host.lock().await).err();
        *self.workspace.lock().await = workspace;
        *self.initialization_error.lock().await = error;
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::INCREMENTAL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions::default()),
                references_provider: Some(OneOf::Left(true)),
                rename_provider: Some(OneOf::Right(editor_types::RenameOptions {
                    prepare_provider: Some(true),
                    work_done_progress_options: Default::default(),
                })),
                workspace_symbol_provider: Some(OneOf::Left(true)),
                inlay_hint_provider: Some(OneOf::Left(true)),
                semantic_tokens_provider: Some(
                    editor_types::SemanticTokensServerCapabilities::SemanticTokensOptions(
                        editor_types::SemanticTokensOptions {
                            legend: editor_adapter::semantic_tokens_legend(),
                            full: Some(editor_types::SemanticTokensFullOptions::Bool(true)),
                            range: None,
                            ..Default::default()
                        },
                    ),
                ),
                signature_help_provider: Some(editor_types::SignatureHelpOptions {
                    trigger_characters: Some(vec!["(".into(), ",".into(), ":".into()]),
                    retrigger_characters: Some(vec![",".into(), " ".into()]),
                    ..Default::default()
                }),
                ..ServerCapabilities::default()
            },
            server_info: None,
        })
    }

    async fn initialized(&self, _params: tower_lsp::lsp_types::InitializedParams) {
        self.register_file_watchers().await;
        let fallback_uri = self.workspace.lock().await.error_uri();
        let generation = self.next_diagnostics_generation();
        if let (Some(uri), Some(error)) = (
            fallback_uri.clone(),
            self.initialization_error.lock().await.take(),
        ) {
            self.publish_project_error(uri, error, generation).await;
            return;
        }
        self.schedule_publish(fallback_uri, Duration::ZERO, generation);
    }

    async fn shutdown(&self) -> LspResult<()> {
        self.next_diagnostics_generation();
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = ClientDocumentVersion::new(params.text_document.version).into_project();
        let generation = self.next_diagnostics_generation();
        let mut host = self.host.lock().await;
        host.open_document(
            DocumentUri::new(uri.to_string()),
            params.text_document.text,
            version,
        );
        self.drc_scheduler.inputs_changed();
        drop(host);
        self.schedule_publish(Some(uri), Duration::ZERO, generation);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = ClientDocumentVersion::new(params.text_document.version).into_project();
        let edits =
            sync::to_text_document_edits(&mapping::LspMapper::new(), params.content_changes);
        let mut host = self.host.lock().await;
        let result = host.apply_document_changes_at_version(
            &DocumentUri::new(uri.as_str()),
            &edits,
            version,
        );
        // A stale notification must leave the current diagnostic tasks intact.
        if matches!(&result, Err(ProjectError::StaleDocumentVersion { .. })) {
            return;
        }
        let generation = self.next_diagnostics_generation();
        if result.is_ok() {
            self.drc_scheduler.inputs_changed();
        }
        drop(host);
        match result {
            Ok(_) => {}
            Err(error) => {
                self.publish_project_error(uri, error, generation).await;
                return;
            }
        }
        self.schedule_publish(Some(uri), self.drc_scheduler.debounce_delay(), generation);
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let generation = self.next_diagnostics_generation();
        let mut host = self.host.lock().await;
        host.close_document(&DocumentUri::new(uri.to_string()));
        self.drc_scheduler.inputs_changed();
        drop(host);
        self.schedule_publish(Some(uri), Duration::ZERO, generation);
    }

    async fn did_change_watched_files(&self, params: DidChangeWatchedFilesParams) {
        self.handle_watched_files(params).await;
    }

    async fn hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let document_uri = DocumentUri::new(uri.to_string());
        let position = self
            .adapter
            .source_position(params.text_document_position_params.position);
        let token = self.cancellation_registry.replace(CancellationSlot::Hover);
        let snapshot = self
            .analysis_snapshot_with_token(&token)
            .await
            .map_err(|error| self.adapter.project_error(error))?;
        let Some(hover) = tokio::task::spawn_blocking(move || {
            snapshot.hover_at_with_token(&document_uri, position, &token)
        })
        .await
        .expect("hover worker must finish without panicking")
        .map_err(|error| self.adapter.query_error(error))?
        else {
            return Ok(None);
        };
        Ok(Some(self.adapter.hover(hover)))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let document_uri = DocumentUri::new(uri.to_string());
        let position = self
            .adapter
            .source_position(params.text_document_position_params.position);
        let token = self
            .cancellation_registry
            .replace(CancellationSlot::Definition);
        let snapshot = self
            .analysis_snapshot_with_token(&token)
            .await
            .map_err(|error| self.adapter.project_error(error))?;
        let Some(definition) = tokio::task::spawn_blocking(move || {
            snapshot.definition_at_with_token(&document_uri, position, &token)
        })
        .await
        .expect("definition worker must finish without panicking")
        .map_err(|error| self.adapter.query_error(error))?
        else {
            return Ok(None);
        };
        Ok(self.adapter.definition(definition))
    }

    async fn completion(&self, params: CompletionParams) -> LspResult<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let document_uri = DocumentUri::new(uri.to_string());
        let position = self.adapter.source_position(position);
        let token = self
            .cancellation_registry
            .replace(CancellationSlot::Completion);
        let snapshot = self
            .analysis_snapshot_with_token(&token)
            .await
            .map_err(|error| self.adapter.project_error(error))?;
        let completions = tokio::task::spawn_blocking(move || {
            snapshot.completions_at_with_token(&document_uri, position, &token)
        })
        .await
        .expect("completion worker must finish without panicking")
        .map_err(|error| self.adapter.query_error(error))?;
        Ok(Some(self.adapter.completion(completions)))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> LspResult<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;
        let document_uri = DocumentUri::new(uri.to_string());
        let token = CancellationToken::new();
        let snapshot = self
            .analysis_snapshot_with_token(&token)
            .await
            .map_err(|error| self.adapter.project_error(error))?;
        let symbols = tokio::task::spawn_blocking(move || snapshot.symbols(&document_uri))
            .await
            .expect("symbol worker must finish without panicking");
        Ok(Some(self.adapter.document_symbols(symbols)))
    }
    async fn references(
        &self,
        params: editor_types::ReferenceParams,
    ) -> LspResult<Option<Vec<editor_types::Location>>> {
        self.handle_references(params).await
    }

    async fn prepare_rename(
        &self,
        params: editor_types::TextDocumentPositionParams,
    ) -> LspResult<Option<editor_types::PrepareRenameResponse>> {
        self.handle_prepare_rename(params).await
    }

    async fn rename(
        &self,
        params: editor_types::RenameParams,
    ) -> LspResult<Option<editor_types::WorkspaceEdit>> {
        self.handle_rename(params).await
    }

    async fn symbol(
        &self,
        params: editor_types::WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<editor_types::SymbolInformation>>> {
        self.handle_symbol(params).await
    }

    async fn inlay_hint(
        &self,
        params: editor_types::InlayHintParams,
    ) -> LspResult<Option<Vec<editor_types::InlayHint>>> {
        self.handle_inlay_hint(params).await
    }

    async fn semantic_tokens_full(
        &self,
        params: editor_types::SemanticTokensParams,
    ) -> LspResult<Option<editor_types::SemanticTokensResult>> {
        self.handle_semantic_tokens_full(params).await
    }

    async fn signature_help(
        &self,
        params: editor_types::SignatureHelpParams,
    ) -> LspResult<Option<editor_types::SignatureHelp>> {
        self.handle_signature_help(params).await
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SylLspServerRunner {
    transport: LspServerTransport,
}

impl SylLspServerRunner {
    pub fn new() -> Self {
        Self::stdio()
    }

    pub fn stdio() -> Self {
        Self {
            transport: LspServerTransport::Stdio,
        }
    }

    pub async fn serve(&self) {
        match self.transport {
            LspServerTransport::Stdio => self.serve_stdio().await,
        }
    }

    async fn serve_stdio(&self) {
        let stdin = tokio::io::stdin();
        let stdout = tokio::io::stdout();
        let (service, socket) = LspService::new(SylLanguageServer::new);
        Server::new(stdin, stdout, socket).serve(service).await;
    }
}

impl Default for SylLspServerRunner {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
enum LspServerTransport {
    Stdio,
}

#[derive(Clone, Copy, Debug)]
struct ClientDocumentVersion {
    raw: i32,
}

impl ClientDocumentVersion {
    fn new(raw: i32) -> Self {
        Self { raw }
    }

    fn into_project(self) -> DocumentVersion {
        match u64::try_from(self.raw) {
            Ok(value) => DocumentVersion::new(value),
            Err(_) => DocumentVersion::zero(),
        }
    }
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod protocol_tests;
