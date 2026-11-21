//! Two diagnostic lanes share a generation and an immutable snapshot.
mod pipeline;
#[cfg(test)]
mod tests;

use crate::cancellation::{CancellationRegistry, CancellationSlot};
use std::{
    future::Future,
    sync::{Arc, Mutex as StdMutex},
    time::Duration,
};
use syl_session::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectError};
use tokio::sync::Mutex;

#[derive(Debug, Default)]
struct GenerationState {
    generation: u64,
    snapshot: Option<Arc<AnalysisSnapshot>>,
}

#[derive(Debug, Default)]
pub(crate) struct BackgroundDrcScheduler {
    state: StdMutex<GenerationState>,
}

impl BackgroundDrcScheduler {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn next_generation(&self, registry: &CancellationRegistry) -> u64 {
        let mut state = self.state.lock().expect("diagnostic generation is usable");
        state.generation += 1;
        state.snapshot = None;
        // Installation of a lane token uses the same lock. A superseded task
        // cannot install an uncancelled token after this invalidation.
        registry.cancel(CancellationSlot::Diagnostics);
        registry.cancel(CancellationSlot::BackgroundElaboration);
        state.generation
    }

    pub(crate) fn generation(&self) -> u64 {
        self.state
            .lock()
            .expect("diagnostic generation is usable")
            .generation
    }

    pub(crate) fn is_current(&self, generation: u64) -> bool {
        self.generation() == generation
    }

    /// Call while holding the host after a mutation. An interactive request
    /// may have cached the prior inputs between invalidation and that lock.
    pub(crate) fn inputs_changed(&self) {
        self.state
            .lock()
            .expect("diagnostic generation is usable")
            .snapshot = None;
    }

    pub(crate) fn token_for(
        &self,
        generation: u64,
        slot: CancellationSlot,
        registry: &CancellationRegistry,
    ) -> Option<CancellationToken> {
        let state = self.state.lock().expect("diagnostic generation is usable");
        (state.generation == generation).then(|| registry.replace(slot))
    }

    pub(crate) fn debounce_delay(&self) -> Duration {
        Duration::from_millis(150)
    }
    pub(crate) fn drc_debounce_delay(&self) -> Duration {
        Duration::from_millis(350)
    }

    fn cached(&self, generation: u64) -> Option<Arc<AnalysisSnapshot>> {
        let state = self.state.lock().expect("diagnostic generation is usable");
        if state.generation != generation {
            return None;
        }
        state.snapshot.clone()
    }

    pub(crate) async fn snapshot(
        self: &Arc<Self>,
        host: Arc<Mutex<AnalysisHost>>,
        generation: u64,
        token: CancellationToken,
    ) -> Result<Arc<AnalysisSnapshot>, ProjectError> {
        if token.is_cancelled() || !self.is_current(generation) {
            return Err(ProjectError::Cancelled);
        }
        if let Some(snapshot) = self.cached(generation) {
            return Ok(snapshot);
        }
        let scheduler = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let mut host = host.blocking_lock();
            if token.is_cancelled() || !scheduler.is_current(generation) {
                return Err(ProjectError::Cancelled);
            }
            if let Some(snapshot) = scheduler.cached(generation) {
                return Ok(snapshot);
            }
            let snapshot = Arc::new(host.snapshot_with_token(&token)?);
            let mut state = scheduler
                .state
                .lock()
                .expect("diagnostic generation is usable");
            if token.is_cancelled() || state.generation != generation {
                return Err(ProjectError::Cancelled);
            }
            state.snapshot = Some(Arc::clone(&snapshot));
            Ok(snapshot)
        })
        .await
        .expect("snapshot worker must finish without panicking")
    }

    pub(crate) async fn run_debounced<F, Fut>(
        self: Arc<Self>,
        generation: u64,
        delay: Duration,
        token: &CancellationToken,
        run: F,
    ) where
        F: FnOnce() -> Fut,
        Fut: Future<Output = ()>,
    {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        if !token.is_cancelled() && self.is_current(generation) {
            run().await;
        }
    }
}
