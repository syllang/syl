use super::*;
use crate::protocol_tests::TestClient;
use serde_json::json;
use syl_query::QueryError;
use tower_lsp::lsp_types::{TextDocumentContentChangeEvent, VersionedTextDocumentIdentifier};

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

#[tokio::test]
async fn stale_did_change_preserves_generation_lane_tokens_and_cached_snapshot() {
    let (service, _socket) = LspService::new(SylLanguageServer::new);
    let server = service.inner();
    let uri = Url::parse("untitled:syl/stale_state").unwrap();
    let document_uri = DocumentUri::new(uri.as_str());
    server.host.lock().await.open_document(
        document_uri.clone(),
        "cell Current() {}\n".into(),
        DocumentVersion::new(2),
    );
    let generation = server.next_diagnostics_generation();
    let snapshot = server
        .analysis_snapshot_with_token(&CancellationToken::new())
        .await
        .unwrap();
    let fast = server
        .cancellation_registry
        .replace(CancellationSlot::Diagnostics);
    let background = server
        .cancellation_registry
        .replace(CancellationSlot::BackgroundElaboration);
    let revision = server.host.lock().await.database().revision();
    for version in [1, 2] {
        server
            .did_change(DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "cell Stale(".into(),
                }],
            })
            .await;
        assert_eq!(server.drc_scheduler.generation(), generation);
        assert!(!fast.is_cancelled());
        assert!(!background.is_cancelled());
        let cached = server
            .analysis_snapshot_with_token(&CancellationToken::new())
            .await
            .unwrap();
        assert!(Arc::ptr_eq(&snapshot, &cached));
        let host = server.host.lock().await;
        let document = host.overlay(&document_uri).unwrap();
        assert_eq!(document.text(), "cell Current() {}\n");
        assert_eq!(document.version(), DocumentVersion::new(2));
        assert_eq!(host.database().revision(), revision);
    }
}

#[tokio::test(start_paused = true)]
async fn stale_v1_during_v2_debounce_preserves_fast_and_background_publication_deadlines() {
    let mut client = TestClient::new();
    client.initialize(json!({"capabilities":{}})).await;
    client.settle().await;
    let uri = "untitled:syl/stale_debounce";
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":uri,"languageId":"syl","version":1,"text":"cell Top(y: out Bit) { y := 0 }\n"}}})).await;
    assert_eq!(client.recv().await["params"]["version"], 1);
    client.settle().await;
    let change = |version, text| json!({"jsonrpc":"2.0","method":"textDocument/didChange","params":{"textDocument":{"uri":uri,"version":version},"contentChanges":[{"text":text}]}});
    client
        .send(change(2, "cell Top(y: out Bit) {\n y := 0\n y := 1\n}\n"))
        .await;
    client.settle().await;
    let generation = client.scheduler.generation();
    tokio::time::advance(Duration::from_millis(75)).await;
    client.send(change(1, "cell Stale(")).await;
    client.settle().await;
    assert_eq!(client.scheduler.generation(), generation);
    tokio::time::advance(Duration::from_millis(74)).await;
    client.settle().await;
    assert!(client.try_recv().is_none());
    tokio::time::advance(Duration::from_millis(1)).await;
    let fast = client.recv().await;
    assert_eq!(fast["method"], "textDocument/publishDiagnostics");
    assert_eq!(fast["params"]["version"], 2);
    assert!(
        fast["params"]["diagnostics"].as_array().unwrap().is_empty(),
        "{fast}"
    );
    client.settle().await;
    tokio::time::advance(Duration::from_millis(349)).await;
    client.settle().await;
    assert!(client.try_recv().is_none());
    tokio::time::advance(Duration::from_millis(1)).await;
    let background = client.recv().await;
    assert_eq!(background["params"]["version"], 2);
    assert!(
        background["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
    );
}
