use super::BackgroundDrcScheduler;
use crate::{
    SylLanguageServer,
    adapter::LspAdapter,
    cancellation::{CancellationRegistry, CancellationSlot},
    diagnostics::{LspDiagnosticPublication, LspDiagnosticState},
};
use std::{sync::Arc, time::Duration};
use syl_query::{AnalysisQueries, QueryError};
use syl_session::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectError};
use tokio::sync::Mutex;
use tower_lsp::{
    Client,
    lsp_types::{MessageType, Url},
};

struct PublishRequest {
    client: Client,
    host: Arc<Mutex<AnalysisHost>>,
    diagnostics: Arc<Mutex<LspDiagnosticState>>,
    scheduler: Arc<BackgroundDrcScheduler>,
    registry: Arc<CancellationRegistry>,
    generation: u64,
    fallback_uri: Option<Url>,
}

impl SylLanguageServer {
    pub(crate) fn schedule_publish(
        &self,
        fallback_uri: Option<Url>,
        delay: Duration,
        generation: u64,
    ) {
        let Some(token) = self.drc_scheduler.token_for(
            generation,
            CancellationSlot::Diagnostics,
            &self.cancellation_registry,
        ) else {
            return;
        };
        let request = PublishRequest {
            client: self.client.clone(),
            host: Arc::clone(&self.host),
            diagnostics: Arc::clone(&self.diagnostics),
            scheduler: Arc::clone(&self.drc_scheduler),
            registry: Arc::clone(&self.cancellation_registry),
            generation,
            fallback_uri,
        };
        tokio::spawn(async move {
            let check_token = token.clone();
            Arc::clone(&request.scheduler)
                .run_debounced(generation, delay, &check_token, || request.run_fast(token))
                .await;
        });
    }

    pub(crate) fn diagnostic_publications(
        snapshot: &AnalysisSnapshot,
        token: &CancellationToken,
    ) -> Result<Vec<LspDiagnosticPublication>, QueryError> {
        let grouped = snapshot.grouped_diagnostics_with_token(token)?;
        Ok(LspAdapter::new().diagnostic_publications(&grouped))
    }
}

impl PublishRequest {
    fn current(&self, token: &CancellationToken) -> bool {
        !token.is_cancelled() && self.scheduler.is_current(self.generation)
    }

    async fn run_fast(self, token: CancellationToken) {
        let snapshot = match self
            .scheduler
            .snapshot(Arc::clone(&self.host), self.generation, token.clone())
            .await
        {
            Ok(snapshot) => snapshot,
            Err(ProjectError::Cancelled) => return,
            Err(error) => {
                let mut state = self.diagnostics.lock().await;
                if !self.current(&token) {
                    return;
                }
                if let Some(uri) = self.fallback_uri.clone() {
                    for publication in state
                        .record_project_error(LspDiagnosticPublication::project_error(uri, error))
                    {
                        let (uri, diagnostics, version) = publication.into_parts();
                        self.client
                            .publish_diagnostics(uri, diagnostics, version)
                            .await;
                    }
                }
                return;
            }
        };
        if !self.current(&token) {
            return;
        }
        let query_snapshot = Arc::clone(&snapshot);
        let query_token = token.clone();
        let result = tokio::task::spawn_blocking(move || {
            SylLanguageServer::diagnostic_publications(&query_snapshot, &query_token)
        })
        .await;
        let publications = match result {
            Ok(Ok(publications)) => publications,
            Ok(Err(QueryError::Cancelled)) => return,
            other => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Syl fast diagnostic worker failed: {other:?}"),
                    )
                    .await;
                return;
            }
        };
        if !self.publish_layer(publications, false, &token).await {
            return;
        }
        let Some(drc_token) = self.scheduler.token_for(
            self.generation,
            CancellationSlot::BackgroundElaboration,
            &self.registry,
        ) else {
            return;
        };
        tokio::spawn(async move {
            let delay = self.scheduler.drc_debounce_delay();
            let check_token = drc_token.clone();
            Arc::clone(&self.scheduler)
                .run_debounced(self.generation, delay, &check_token, || {
                    self.run_background(snapshot, drc_token)
                })
                .await;
        });
    }

    async fn run_background(self, snapshot: Arc<AnalysisSnapshot>, token: CancellationToken) {
        tokio::task::yield_now().await;
        if !self.current(&token) {
            return;
        }
        let query_token = token.clone();
        let result = tokio::task::spawn_blocking(move || {
            snapshot
                .background_elaboration_diagnostics_with_token(&query_token)
                .map(|grouped| LspAdapter::new().background_elaboration_publications(&grouped))
        })
        .await;
        match result {
            Ok(Ok(publications)) => {
                self.publish_layer(publications, true, &token).await;
            }
            Ok(Err(QueryError::Cancelled)) => {}
            other => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Syl background diagnostic worker failed: {other:?}"),
                    )
                    .await
            }
        }
    }

    async fn publish_layer(
        &self,
        publications: Vec<LspDiagnosticPublication>,
        background: bool,
        token: &CancellationToken,
    ) -> bool {
        // Serialize reconciliation and transport so a slow old send cannot
        // overtake a newer publication after the state lock is released.
        let mut state = self.diagnostics.lock().await;
        if !self.current(token) {
            return false;
        }
        let publications = if background {
            state.apply_elaboration_layer(self.generation, publications)
        } else {
            state.apply_fast_layer(self.generation, publications)
        };
        for publication in publications {
            if !self.current(token) {
                return false;
            }
            let (uri, diagnostics, version) = publication.into_parts();
            self.client
                .publish_diagnostics(uri, diagnostics, version)
                .await;
        }
        self.current(token)
    }
}
