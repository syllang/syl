use super::*;
use crate::{SylLanguageServer, protocol_tests::TestClient};
use serde_json::json;
use syl_session::{AnalysisHost, DocumentUri, DocumentVersion, ProjectError};
use tower_lsp::{LanguageServer, LspService, lsp_types::*};

#[test]
fn incremental_adapter_preserves_utf16_coordinates_and_full_fallback() {
    let uri = DocumentUri::new("untitled:syl/edit");
    let changes = vec![
        TextDocumentContentChangeEvent {
            range: None,
            range_length: None,
            text: "a💡\r\nb".into(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 3), Position::new(1, 0))),
            range_length: Some(2),
            text: "".into(),
        },
        TextDocumentContentChangeEvent {
            range: Some(Range::new(Position::new(0, 1), Position::new(0, 3))),
            range_length: None,
            text: "!".into(),
        },
    ];
    let edits = to_text_document_edits(&LspMapper::new(), changes);
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), "".into(), DocumentVersion::new(2));
    host.apply_document_changes_at_version(&uri, &edits, DocumentVersion::new(3))
        .unwrap();
    assert_eq!(host.overlay(&uri).unwrap().text(), "a!b");
    assert!(matches!(
        host.apply_document_changes_at_version(&uri, &edits, DocumentVersion::new(3)),
        Err(ProjectError::StaleDocumentVersion { .. })
    ));
}

#[tokio::test(start_paused = true)]
async fn did_change_applies_all_events_and_ignores_stale_version() {
    let (service, _socket) = LspService::new(SylLanguageServer::new);
    let server = service.inner();
    let uri = Url::parse("untitled:syl/edit").unwrap();
    let capabilities = server
        .initialize(InitializeParams::default())
        .await
        .unwrap()
        .capabilities;
    assert_eq!(
        capabilities.text_document_sync,
        Some(TextDocumentSyncCapability::Kind(
            TextDocumentSyncKind::INCREMENTAL
        ))
    );
    server
        .did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem::new(uri.clone(), "syl".into(), 4, "abc".into()),
        })
        .await;
    let params = |version| DidChangeTextDocumentParams {
        text_document: VersionedTextDocumentIdentifier::new(uri.clone(), version),
        content_changes: vec![
            TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 1), Position::new(0, 2))),
                range_length: None,
                text: "XYZ".into(),
            },
            TextDocumentContentChangeEvent {
                range: Some(Range::new(Position::new(0, 4), Position::new(0, 5))),
                range_length: None,
                text: "!".into(),
            },
        ],
    };
    server.did_change(params(5)).await;
    server.did_change(params(4)).await;
    let host = server.host.lock().await;
    let overlay = host.overlay(&DocumentUri::new(uri.as_str())).unwrap();
    assert_eq!(overlay.text(), "aXYZ!");
    assert_eq!(overlay.version(), DocumentVersion::new(5));
    server.next_diagnostics_generation();
}

#[tokio::test(start_paused = true)]
async fn wire_incremental_capability_and_full_fallback_publish_current_version() {
    let mut client = TestClient::new();
    let response = client.initialize(json!({"capabilities":{}})).await;
    assert_eq!(response["result"]["capabilities"]["textDocumentSync"], 2);
    client.send(json!({"jsonrpc":"2.0","method":"textDocument/didOpen","params":{"textDocument":{"uri":"untitled:syl/wire","languageId":"syl","version":1,"text":"cell Broken("}}})).await;
    let publication = client.recv().await;
    assert_eq!(publication["method"], "textDocument/publishDiagnostics");
    assert_eq!(publication["params"]["version"], 1);
    assert!(
        !publication["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
