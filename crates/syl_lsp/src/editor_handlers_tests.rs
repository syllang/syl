use super::*;
use syl_session::DocumentVersion;
use tower_lsp::{LanguageServer, LspService, jsonrpc::ErrorCode};

#[tokio::test]
async fn capabilities_and_all_editor_requests() {
    let (service, _socket) = LspService::new(SylLanguageServer::new);
    let server = service.inner();
    let caps = server
        .initialize(InitializeParams::default())
        .await
        .unwrap()
        .capabilities;
    assert_eq!(caps.references_provider, Some(OneOf::Left(true)));
    assert!(matches!(
        caps.rename_provider,
        Some(OneOf::Right(RenameOptions {
            prepare_provider: Some(true),
            ..
        }))
    ));
    assert_eq!(caps.workspace_symbol_provider, Some(OneOf::Left(true)));
    assert_eq!(caps.inlay_hint_provider, Some(OneOf::Left(true)));
    assert!(caps.semantic_tokens_provider.is_some());
    assert_eq!(
        caps.signature_help_provider
            .unwrap()
            .trigger_characters
            .unwrap(),
        ["(", ",", ":"]
    );
    let uri = Url::parse("file:///handlers.syl").unwrap();
    let text = "fn helper(x: nat) -> nat { let value = x; value }\nfn main() -> nat { helper(1) }";
    server.host.lock().await.open_document(
        DocumentUri::new(uri.as_str()),
        text.into(),
        DocumentVersion::new(1),
    );
    let document = TextDocumentIdentifier::new(uri.clone());
    let at = TextDocumentPositionParams::new(document.clone(), Position::new(0, 4));
    let refs = server
        .references(ReferenceParams {
            text_document_position: at.clone(),
            context: ReferenceContext {
                include_declaration: true,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refs.len(), 2);
    assert!(server.prepare_rename(at.clone()).await.unwrap().is_some());
    let rename = server
        .rename(RenameParams {
            text_document_position: at.clone(),
            new_name: "renamed".into(),
            work_done_progress_params: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(rename.changes.unwrap()[&uri].len(), 2);
    let error = server
        .rename(RenameParams {
            text_document_position: at,
            new_name: "cell".into(),
            work_done_progress_params: Default::default(),
        })
        .await
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::InvalidParams);
    let symbols = server
        .symbol(WorkspaceSymbolParams {
            query: "help".into(),
            ..Default::default()
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(symbols[0].name, "helper");
    let hints = server
        .inlay_hint(InlayHintParams {
            text_document: document.clone(),
            range: Range::new(Position::new(0, 0), Position::new(5, 0)),
            work_done_progress_params: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();
    assert!(
        hints
            .iter()
            .any(|h| matches!(&h.label, InlayHintLabel::String(s) if s == ": nat"))
    );
    assert!(
        server
            .semantic_tokens_full(SemanticTokensParams {
                text_document: document.clone(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default()
            })
            .await
            .unwrap()
            .is_some()
    );
    let help = server
        .signature_help(SignatureHelpParams {
            text_document_position_params: TextDocumentPositionParams::new(
                document,
                Position::new(1, 26),
            ),
            context: None,
            work_done_progress_params: Default::default(),
        })
        .await
        .unwrap()
        .unwrap();
    assert_eq!(help.active_parameter, Some(0));
}

#[tokio::test]
async fn superseded_blocking_editor_query_is_cancelled() {
    let (service, _socket) = LspService::new(SylLanguageServer::new);
    let server = service.inner();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let first = server.editor_query(CancellationSlot::References, move |_, token| {
        started_tx.send(()).unwrap();
        while !token.is_cancelled() {
            std::thread::yield_now();
        }
        Err::<(), _>(QueryError::Cancelled)
    });
    let replacement = async {
        started_rx.await.unwrap();
        server
            .editor_query(CancellationSlot::References, |_, _| Ok(()))
            .await
            .unwrap();
    };
    let (first, ()) = tokio::join!(first, replacement);
    assert_eq!(first.unwrap_err().code, ErrorCode::RequestCancelled);
}
