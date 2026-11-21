use super::*;
use crate::{SylLanguageServer, protocol_tests::TestClient};
use serde_json::json;
use std::sync::{
    Barrier,
    atomic::{AtomicBool, Ordering},
};
use syl_query::AnalysisQueries;
use syl_session::{DocumentUri, DocumentVersion};
use tower_lsp::{LanguageServer, LspService, lsp_types::*};

#[tokio::test(start_paused = true)]
async fn debounce_runs_current_generation_only_and_invalidation_cancels_both_lanes() {
    let scheduler = Arc::new(BackgroundDrcScheduler::new());
    let registry = CancellationRegistry::new();
    let generation = scheduler.next_generation(&registry);
    let fast = scheduler
        .token_for(generation, CancellationSlot::Diagnostics, &registry)
        .unwrap();
    let drc = scheduler
        .token_for(
            generation,
            CancellationSlot::BackgroundElaboration,
            &registry,
        )
        .unwrap();
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    let old = Arc::clone(&scheduler);
    let task = tokio::spawn(async move {
        Arc::clone(&old)
            .run_debounced(generation, old.debounce_delay(), &fast, || async move {
                flag.store(true, Ordering::Release);
            })
            .await;
    });
    scheduler.next_generation(&registry);
    assert!(drc.is_cancelled());
    assert!(
        scheduler
            .token_for(
                generation,
                CancellationSlot::BackgroundElaboration,
                &registry
            )
            .is_none()
    );
    tokio::time::advance(Duration::from_millis(500)).await;
    task.await.unwrap();
    assert!(!ran.load(Ordering::Acquire));
    let flag = Arc::clone(&ran);
    Arc::clone(&scheduler)
        .run_debounced(
            scheduler.generation(),
            Duration::ZERO,
            &CancellationToken::new(),
            || async move {
                flag.store(true, Ordering::Release);
            },
        )
        .await;
    assert!(ran.load(Ordering::Acquire));
}

#[tokio::test(start_paused = true)]
async fn fast_lane_precedes_drc_and_edit_cancels_the_old_background_generation() {
    let mut client = TestClient::new();
    client.initialize(json!({"capabilities":{}})).await;
    client.settle().await;
    let uri = "untitled:syl/drc";
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"syl","version":1,"text":"cell Top(y: out Bit) {\n y := 0\n y := 1\n}\n"}}})).await;
    let fast = client.recv().await;
    assert_eq!(fast["params"]["version"], 1);
    assert!(fast["params"]["diagnostics"].as_array().unwrap().is_empty());
    client.settle().await;
    tokio::time::advance(Duration::from_millis(349)).await;
    client.settle().await;
    assert!(client.try_recv().is_none());
    tokio::time::advance(Duration::from_millis(1)).await;
    let drc = client.recv().await;
    assert_eq!(drc["params"]["version"], 1);
    assert!(
        drc["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
    );
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"range":{"start":{"line":2,"character":0},"end":{"line":3,"character":0}},"text":""}]}})).await;
    client.settle().await;
    tokio::time::advance(Duration::from_millis(149)).await;
    client.settle().await;
    assert!(client.try_recv().is_none());
    tokio::time::advance(Duration::from_millis(1)).await;
    let fixed = client.recv().await;
    assert_eq!(fixed["params"]["version"], 2);
    assert!(
        fixed["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    client.settle().await;
    tokio::time::advance(Duration::from_millis(350)).await;
    client.settle().await;
    assert!(client.try_recv().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn navigation_uses_the_shared_snapshot_while_background_worker_owns_it() {
    let (service, _socket) = LspService::new(SylLanguageServer::new);
    let server = service.inner();
    let uri = DocumentUri::new("untitled:syl/concurrent");
    server.host.lock().await.open_document(
        uri.clone(),
        "cell Top(x: in Bit, y: out Bit) { y := x }\n".into(),
        DocumentVersion::new(1),
    );
    let generation = server.next_diagnostics_generation();
    let snapshot = server
        .analysis_snapshot_with_token(&CancellationToken::new())
        .await
        .unwrap();
    let another = server
        .analysis_snapshot_with_token(&CancellationToken::new())
        .await
        .unwrap();
    assert!(Arc::ptr_eq(&snapshot, &another));
    let start = Arc::new(Barrier::new(2));
    let end = Arc::new(Barrier::new(2));
    let entered = Arc::clone(&start);
    let release = Arc::clone(&end);
    let drc_snapshot = Arc::clone(&snapshot);
    let token = server
        .drc_scheduler
        .token_for(
            generation,
            CancellationSlot::BackgroundElaboration,
            &server.cancellation_registry,
        )
        .unwrap();
    let worker = tokio::task::spawn_blocking(move || {
        entered.wait();
        let result = drc_snapshot.background_elaboration_diagnostics_with_token(&token);
        release.wait();
        result.unwrap()
    });
    tokio::task::spawn_blocking(move || start.wait())
        .await
        .unwrap();
    // Any attempt to re-publish session inputs would wait on this host guard.
    let host_guard = server.host.lock().await;
    let position = TextDocumentPositionParams {
        text_document: TextDocumentIdentifier::new(Url::parse(uri.as_str()).unwrap()),
        position: Position::new(0, 37),
    };
    let navigation = async {
        assert!(
            server
                .hover(HoverParams {
                    text_document_position_params: position.clone(),
                    work_done_progress_params: Default::default()
                })
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            server
                .goto_definition(GotoDefinitionParams {
                    text_document_position_params: position.clone(),
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default()
                })
                .await
                .unwrap()
                .is_some()
        );
        assert!(
            server
                .completion(CompletionParams {
                    text_document_position: position,
                    work_done_progress_params: Default::default(),
                    partial_result_params: Default::default(),
                    context: None
                })
                .await
                .unwrap()
                .is_some()
        );
    };
    let result = tokio::time::timeout(Duration::from_secs(2), navigation).await;
    end.wait();
    worker.await.unwrap();
    drop(host_guard);
    assert!(
        result.is_ok(),
        "navigation waited for the background worker or host lock"
    );
}

#[tokio::test(start_paused = true)]
async fn editing_during_drc_debounce_never_publishes_the_superseded_driver_error() {
    let mut client = TestClient::new();
    client.initialize(json!({"capabilities":{}})).await;
    client.settle().await;
    let uri = "untitled:syl/debounced";
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"syl","version":1,"text":"cell Top(y: out Bit) {\n y := 0\n y := 1\n}\n"}}})).await;
    assert_eq!(client.recv().await["params"]["version"], 1);
    client.settle().await;
    tokio::time::advance(Duration::from_millis(100)).await;
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":2},"contentChanges":[{"text":"cell Top(y: out Bit) { y := 0 }\n"}]}})).await;
    client.settle().await;
    tokio::time::advance(Duration::from_millis(150)).await;
    let fast = client.recv().await;
    assert_eq!(fast["params"]["version"], 2);
    assert!(fast["params"]["diagnostics"].as_array().unwrap().is_empty());
    client.settle().await;
    tokio::time::advance(Duration::from_millis(100)).await;
    client.settle().await;
    assert!(
        client.try_recv().is_none(),
        "old DRC published at its old deadline"
    );
    tokio::time::advance(Duration::from_millis(250)).await;
    client.settle().await;
    assert!(client.try_recv().is_none());
}

#[tokio::test]
async fn mutation_invalidates_a_snapshot_cached_before_the_host_lock_was_acquired() {
    let scheduler = Arc::new(BackgroundDrcScheduler::new());
    let registry = CancellationRegistry::new();
    let uri = DocumentUri::new("untitled:syl/race");
    let mut host = AnalysisHost::new();
    host.open_document(
        uri.clone(),
        "cell Before() {}".into(),
        DocumentVersion::new(1),
    );
    let host = Arc::new(Mutex::new(host));
    let generation = scheduler.next_generation(&registry);
    let earlier = scheduler
        .snapshot(Arc::clone(&host), generation, CancellationToken::new())
        .await
        .unwrap();
    {
        let mut host = host.lock().await;
        host.update_document(&uri, "cell After() {}".into())
            .unwrap();
        scheduler.inputs_changed();
    }
    let current = scheduler
        .snapshot(host, generation, CancellationToken::new())
        .await
        .unwrap();
    assert!(!Arc::ptr_eq(&earlier, &current));
    assert_eq!(
        current.file_by_uri(&uri).unwrap().version(),
        DocumentVersion::new(2)
    );
    assert_eq!(
        earlier.file_by_uri(&uri).unwrap().version(),
        DocumentVersion::new(1)
    );
}
