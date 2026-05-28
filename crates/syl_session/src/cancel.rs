use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// A cooperative cancellation token for interrupting long-running analysis.
///
/// **Cooperative, not preemptive:** The token only signals *intent* to cancel.
/// Code must periodically check `is_cancelled()` and return early. The token
/// does NOT abort threads or force-close resources.
///
/// **Clone behavior:** Cloning shares the same underlying `AtomicBool`.
/// Calling `cancel()` on any clone signals all clones.
///
/// **Typical usage:** `AnalysisDatabase::load` checks `cancellation.is_cancelled()`
/// between compilation stages. Most hot loops do NOT check the token (by design)
/// to avoid perf impact — cancellation is checked at stage boundaries.
///
/// ```ignore
/// let token = CancellationToken::new();
/// let worker = thread::spawn(move || {
///     while !token.is_cancelled() {
///         do_work_chunk();
///     }
/// });
/// token.cancel();  // worker will stop at next check
/// ```
#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct CancellationToken {
    cancelled: Arc<AtomicBool>,
}

impl CancellationToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }
}
