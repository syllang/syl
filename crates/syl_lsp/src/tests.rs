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
        let first_uri = DocumentUri::new(format!("untitled:syl/alpha{attempt}"));
        let second_uri = DocumentUri::new(format!("untitled:syl/beta{attempt}"));
        let mut host = AnalysisHost::new();
        host.open_document(
            first_uri,
            "module Alpha(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        host.open_document(
            second_uri,
            "module Beta(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        let snapshot = host
            .snapshot()
            .expect("LSP cancellation fixture must snapshot cleanly");
        let token = CancellationToken::new();
        token.cancel();

        let err = SylLanguageServer::diagnostic_publications(&snapshot, &token)
            .expect_err("cancelled diagnostic publication must not publish stale packages");

        assert_eq!(err, QueryError::Cancelled);
        SylLanguageServer::diagnostic_publications(&snapshot, &CancellationToken::new())
            .expect("retry after cancel returns publications");
    }
}
