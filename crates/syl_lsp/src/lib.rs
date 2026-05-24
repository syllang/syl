mod adapter;
mod cancellation;
mod diagnostics;
mod mapping;

use adapter::LspAdapter;
use cancellation::{CancellationRegistry, CancellationSlot};
use diagnostics::{LspDiagnosticPublication, LspDiagnosticState};
use std::{
    future::Future,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::Duration,
};
use syl_query::{AnalysisQueries, QueryError};
use syl_session::{
    AnalysisHost, AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectError,
};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tower_lsp::{
    Client, LanguageServer, LspService, Server,
    jsonrpc::Result as LspResult,
    lsp_types::{
        CompletionOptions, CompletionParams, CompletionResponse, DidChangeTextDocumentParams,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DocumentSymbolParams,
        DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams,
        InitializeParams, InitializeResult, OneOf, ServerCapabilities, TextDocumentSyncCapability,
        TextDocumentSyncKind, Url,
    },
};

#[derive(Debug)]
#[non_exhaustive]
pub struct SylLanguageServer {
    client: Client,
    host: Arc<Mutex<AnalysisHost>>,
    diagnostics: Arc<Mutex<LspDiagnosticState>>,
    workspace_diagnostic_uri: Arc<Mutex<Option<Url>>>,
    initialization_error: Arc<Mutex<Option<ProjectError>>>,
    diagnostic_scheduler: Arc<DiagnosticsScheduler>,
    cancellation_registry: Arc<CancellationRegistry>,
    adapter: LspAdapter,
}

struct DiagnosticPublishRequest {
    client: Client,
    host: Arc<Mutex<AnalysisHost>>,
    diagnostics: Arc<Mutex<LspDiagnosticState>>,
    generation: u64,
    scheduler: Arc<DiagnosticsScheduler>,
    fallback_uri: Option<Url>,
    token: CancellationToken,
}

impl SylLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            host: Arc::new(Mutex::new(AnalysisHost::new())),
            diagnostics: Arc::new(Mutex::new(LspDiagnosticState::new())),
            workspace_diagnostic_uri: Arc::new(Mutex::new(None)),
            initialization_error: Arc::new(Mutex::new(None)),
            diagnostic_scheduler: Arc::new(DiagnosticsScheduler::new()),
            cancellation_registry: Arc::new(CancellationRegistry::new()),
            adapter: LspAdapter::new(),
        }
    }

    async fn analysis_snapshot_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<AnalysisSnapshot, ProjectError> {
        let mut host = self.host.lock().await;
        host.snapshot_with_token(token)
    }

    async fn publish_project_error(&self, uri: Url, error: ProjectError) {
        let publication = LspDiagnosticPublication::project_error(uri, error);
        let publications = self
            .diagnostics
            .lock()
            .await
            .record_project_error(publication);
        for publication in publications {
            let (target_uri, diagnostics, version) = publication.into_parts();
            self.client
                .publish_diagnostics(target_uri, diagnostics, version)
                .await;
        }
    }

    fn schedule_publish(&self, fallback_uri: Option<Url>, delay: Duration, generation: u64) {
        let client = self.client.clone();
        let host = Arc::clone(&self.host);
        let diagnostics = Arc::clone(&self.diagnostics);
        let scheduler = Arc::clone(&self.diagnostic_scheduler);
        let token = self
            .cancellation_registry
            .replace(CancellationSlot::Diagnostics);
        let request = DiagnosticPublishRequest {
            client,
            host,
            diagnostics,
            generation,
            scheduler: Arc::clone(&scheduler),
            fallback_uri,
            token,
        };
        tokio::spawn(async move {
            Self::run_debounced_publish(scheduler, generation, delay, move || async move {
                Self::publish_if_current(request).await;
            })
            .await;
        });
    }

    async fn run_debounced_publish<F, Fut>(
        scheduler: Arc<DiagnosticsScheduler>,
        generation: u64,
        delay: Duration,
        publish: F,
    ) where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        if !delay.is_zero() {
            sleep(delay).await;
        }
        if !scheduler.is_current(generation) {
            return;
        }
        publish().await;
    }

    async fn publish_if_current(request: DiagnosticPublishRequest) {
        let DiagnosticPublishRequest {
            client,
            host,
            diagnostics,
            generation,
            scheduler,
            fallback_uri,
            token,
        } = request;
        if token.is_cancelled() || !scheduler.is_current(generation) {
            return;
        }
        let snapshot = {
            let mut host = host.lock().await;
            host.snapshot_with_token(&token)
        };
        if token.is_cancelled() || !scheduler.is_current(generation) {
            return;
        }
        let publications = match snapshot {
            Ok(snapshot) => match Self::diagnostic_publications(&snapshot, &token) {
                Ok(publications) => publications,
                Err(QueryError::Cancelled) => return,
                Err(error) => {
                    panic!("unexpected query error during diagnostic publication: {error}")
                }
            },
            Err(ProjectError::Cancelled) => return,
            Err(error) => fallback_uri
                .map(|uri| LspDiagnosticPublication::project_error(uri, error))
                .into_iter()
                .collect(),
        };
        if token.is_cancelled() || !scheduler.is_current(generation) {
            return;
        }
        let publications = diagnostics.lock().await.reconcile(publications);
        if token.is_cancelled() || !scheduler.is_current(generation) {
            return;
        }
        for publication in publications {
            if token.is_cancelled() || !scheduler.is_current(generation) {
                return;
            }
            let (target_uri, diagnostics, version) = publication.into_parts();
            client
                .publish_diagnostics(target_uri, diagnostics, version)
                .await;
        }
    }

    fn diagnostic_publications(
        snapshot: &AnalysisSnapshot,
        token: &CancellationToken,
    ) -> Result<Vec<LspDiagnosticPublication>, QueryError> {
        let grouped = snapshot.grouped_diagnostics_with_token(token)?;
        Ok(LspAdapter::new().diagnostic_publications(&grouped))
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for SylLanguageServer {
    async fn initialize(&self, params: InitializeParams) -> LspResult<InitializeResult> {
        let workspace = WorkspaceInitialization::new(&params);
        *self.workspace_diagnostic_uri.lock().await = workspace.fallback_uri();
        let roots = workspace.into_roots();
        if !roots.is_empty() {
            *self.initialization_error.lock().await = self.host.lock().await.load(&roots).err();
        }
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                hover_provider: Some(tower_lsp::lsp_types::HoverProviderCapability::Simple(true)),
                definition_provider: Some(OneOf::Left(true)),
                document_symbol_provider: Some(OneOf::Left(true)),
                completion_provider: Some(CompletionOptions::default()),
                ..ServerCapabilities::default()
            },
            server_info: None,
        })
    }

    async fn initialized(&self, _params: tower_lsp::lsp_types::InitializedParams) {
        let fallback_uri = self.workspace_diagnostic_uri.lock().await.clone();
        let generation = self.diagnostic_scheduler.next_generation();
        if let (Some(uri), Some(error)) = (
            fallback_uri.clone(),
            self.initialization_error.lock().await.take(),
        ) {
            self.publish_project_error(uri, error).await;
            return;
        }
        self.schedule_publish(fallback_uri, Duration::ZERO, generation);
    }

    async fn shutdown(&self) -> LspResult<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = ClientDocumentVersion::new(params.text_document.version).into_project();
        let generation = self.diagnostic_scheduler.next_generation();
        self.host.lock().await.open_document(
            DocumentUri::new(uri.to_string()),
            params.text_document.text,
            version,
        );
        self.schedule_publish(Some(uri), Duration::ZERO, generation);
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let generation = self.diagnostic_scheduler.next_generation();
        if let Some(change) = params.content_changes.into_iter().last() {
            let version = ClientDocumentVersion::new(params.text_document.version).into_project();
            let result = {
                self.host.lock().await.update_document_at_version(
                    &DocumentUri::new(uri.to_string()),
                    change.text,
                    version,
                )
            };
            match result {
                Ok(_) => {}
                Err(ProjectError::StaleDocumentVersion { .. }) => return,
                Err(error) => {
                    self.publish_project_error(uri, error).await;
                    return;
                }
            }
        }
        self.schedule_publish(
            Some(uri),
            self.diagnostic_scheduler.debounce_delay(),
            generation,
        );
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        let generation = self.diagnostic_scheduler.next_generation();
        self.host
            .lock()
            .await
            .close_document(&DocumentUri::new(uri.to_string()));
        self.schedule_publish(Some(uri), Duration::ZERO, generation);
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
        let Some(hover) = snapshot
            .hover_at_with_token(&document_uri, position, &token)
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
        let Some(definition) = snapshot
            .definition_at_with_token(&document_uri, position, &token)
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
        let completions = snapshot
            .completions_at_with_token(&document_uri, position, &token)
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
        let symbols = snapshot.symbols(&document_uri);
        Ok(Some(self.adapter.document_symbols(symbols)))
    }
}

#[derive(Debug)]
struct DiagnosticsScheduler {
    generation: AtomicU64,
    debounce_delay: Duration,
}

impl DiagnosticsScheduler {
    fn new() -> Self {
        Self {
            generation: AtomicU64::new(0),
            debounce_delay: Duration::from_millis(150),
        }
    }

    fn next_generation(&self) -> u64 {
        self.generation.fetch_add(1, Ordering::AcqRel) + 1
    }

    fn is_current(&self, generation: u64) -> bool {
        self.generation.load(Ordering::Acquire) == generation
    }

    fn debounce_delay(&self) -> Duration {
        self.debounce_delay
    }
}

#[non_exhaustive]
struct WorkspaceInitialization {
    roots: Vec<PathBuf>,
    fallback_uri: Option<Url>,
}

impl WorkspaceInitialization {
    fn new(params: &InitializeParams) -> Self {
        let mut initialization = Self {
            roots: Vec::new(),
            fallback_uri: None,
        };
        initialization.add_workspace_folders(params);
        initialization.add_root_uri(params);
        initialization.add_root_path(params);
        initialization
    }

    fn fallback_uri(&self) -> Option<Url> {
        self.fallback_uri.clone()
    }

    fn into_roots(self) -> Vec<PathBuf> {
        self.roots
    }

    fn add_workspace_folders(&mut self, params: &InitializeParams) {
        if let Some(folders) = &params.workspace_folders {
            for folder in folders {
                self.add_uri(&folder.uri);
            }
        }
    }

    fn add_root_uri(&mut self, params: &InitializeParams) {
        if !self.roots.is_empty() {
            return;
        }
        if let Some(uri) = &params.root_uri {
            self.add_uri(uri);
        }
    }

    #[allow(deprecated)]
    fn add_root_path(&mut self, params: &InitializeParams) {
        if !self.roots.is_empty() {
            return;
        }
        if let Some(path) = &params.root_path {
            self.add_path_with_fallback(PathBuf::from(path));
        }
    }

    fn add_uri(&mut self, uri: &Url) {
        let Ok(path) = uri.to_file_path() else {
            return;
        };
        if self.fallback_uri.is_none() {
            self.fallback_uri = Some(uri.clone());
        }
        self.add_path(path);
    }

    fn add_path(&mut self, path: PathBuf) {
        if !self.roots.contains(&path) {
            self.roots.push(path);
        }
    }

    fn add_path_with_fallback(&mut self, path: PathBuf) {
        if self.fallback_uri.is_none()
            && let Ok(uri) = Url::from_file_path(&path)
        {
            self.fallback_uri = Some(uri);
        }
        self.add_path(path);
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
mod tests {
    use super::*;
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    #[tokio::test(start_paused = true)]
    async fn stale_generation_does_not_run_debounced_publish() {
        let scheduler = Arc::new(DiagnosticsScheduler::new());
        let generation = scheduler.next_generation();
        let published = Arc::new(AtomicBool::new(false));
        let publication = Arc::clone(&published);
        let delay = scheduler.debounce_delay();
        let task = tokio::spawn(SylLanguageServer::run_debounced_publish(
            Arc::clone(&scheduler),
            generation,
            delay,
            move || async move {
                publication.store(true, Ordering::Release);
            },
        ));

        scheduler.next_generation();
        tokio::time::advance(delay).await;
        task.await
            .expect("debounced publish task must finish cleanly");

        assert!(!published.load(Ordering::Acquire));
    }

    #[tokio::test(start_paused = true)]
    async fn current_generation_runs_debounced_publish() {
        let scheduler = Arc::new(DiagnosticsScheduler::new());
        let generation = scheduler.next_generation();
        let published = Arc::new(AtomicBool::new(false));
        let publication = Arc::clone(&published);
        let delay = scheduler.debounce_delay();
        let task = tokio::spawn(SylLanguageServer::run_debounced_publish(
            Arc::clone(&scheduler),
            generation,
            delay,
            move || async move {
                publication.store(true, Ordering::Release);
            },
        ));

        tokio::time::advance(delay).await;
        task.await
            .expect("debounced publish task must finish cleanly");

        assert!(published.load(Ordering::Acquire));
    }

    #[test]
    fn new_diagnostics_generation_cancels_previous_token() {
        let registry = CancellationRegistry::new();

        let first = registry.replace(CancellationSlot::Diagnostics);
        let second = registry.replace(CancellationSlot::Diagnostics);

        assert!(first.is_cancelled());
        assert!(!second.is_cancelled());
    }

    #[test]
    fn diagnostic_publications_use_token_aware_grouped_diagnostics() {
        for attempt in 0..20 {
            let first_uri = DocumentUri::new(format!("untitled:syl/lsp-alpha-{attempt}"));
            let second_uri = DocumentUri::new(format!("untitled:syl/lsp-beta-{attempt}"));
            let mut host = AnalysisHost::new();
            host.open_document(
                first_uri,
                "package alpha;\nmodule Alpha(y: out Bit) { y := 1 }\n".to_string(),
                DocumentVersion::new(1),
            );
            host.open_document(
                second_uri,
                "package beta;\nmodule Beta(y: out Bit) { y := 1 }\n".to_string(),
                DocumentVersion::new(1),
            );
            let snapshot = host
                .snapshot()
                .expect("LSP cancellation fixture must snapshot cleanly");
            let alpha_cache = snapshot
                .package_semantic_cache("alpha")
                .expect("alpha package shard must exist");
            let beta_cache = snapshot
                .package_semantic_cache("beta")
                .expect("beta package shard must exist");
            let token = CancellationToken::new();
            token.cancel();

            let err = SylLanguageServer::diagnostic_publications(&snapshot, &token)
                .expect_err("cancelled diagnostic publication must not publish stale packages");

            assert_eq!(err, QueryError::Cancelled);
            assert!(!alpha_cache.is_hir_cached());
            assert!(!beta_cache.is_hir_cached());
            assert!(!beta_cache.is_tir_cached());
            assert!(!beta_cache.is_elaboration_cached());
        }
    }
}
