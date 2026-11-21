use super::*;
use syl_query::EditorQueries;
use syl_session::{AnalysisDatabase, DocumentUri, DocumentVersion};

#[test]
fn delta_encoding_uses_utf16_and_canonical_legend() {
    let uri = DocumentUri::new("file:///tokens.syl");
    let mut db = AnalysisDatabase::new();
    db.open_document(
        uri.clone(),
        "const s = \"💡\"; const x = 1;\n// doc\nconst y = 2;".into(),
        DocumentVersion::new(1),
    );
    let snapshot = db.snapshot().unwrap();
    let tokens = snapshot.semantic_tokens(&uri);
    let encoded = encode_semantic_tokens(tokens.clone());
    let lsp::SemanticTokensResult::Tokens(encoded) = encoded else {
        panic!("full tokens expected")
    };
    assert_eq!(tokens.len(), encoded.data.len());
    let mut line = 0;
    let mut start = 0;
    for (result, token) in tokens.iter().zip(&encoded.data) {
        line += token.delta_line;
        start = if token.delta_line == 0 {
            start + token.delta_start
        } else {
            token.delta_start
        };
        assert_eq!(
            (line as usize, start as usize),
            (result.range.start.line, result.range.start.character)
        );
        assert_eq!(
            token.length as usize,
            result.range.end.character - result.range.start.character
        );
        assert!(token.token_type < 16);
    }
    let string = encoded.data.iter().find(|t| t.token_type == 12).unwrap();
    assert_eq!(string.length, 4);
    assert_eq!(
        semantic_tokens_legend().token_types[12],
        lsp::SemanticTokenType::STRING
    );
    assert_eq!(semantic_tokens_legend().token_modifiers.len(), 6);
    let keyword = &encoded.data[0];
    assert_eq!(
        (
            keyword.delta_line,
            keyword.delta_start,
            keyword.length,
            keyword.token_type
        ),
        (0, 0, 5, 10)
    );
    let constant = &encoded.data[1];
    assert_eq!(
        (
            constant.delta_line,
            constant.delta_start,
            constant.token_modifiers_bitset
        ),
        (0, 6, 7)
    );
    assert!(
        encoded
            .data
            .iter()
            .any(|t| t.delta_line == 1 && t.delta_start == 0 && t.token_type == 14)
    );
}

#[test]
fn rename_mapping_groups_edits_and_maps_errors() {
    let uri = DocumentUri::new("file:///rename.syl");
    let mut db = AnalysisDatabase::new();
    db.open_document(
        uri.clone(),
        "const value = 1; fn f() -> nat { value }".into(),
        DocumentVersion::new(1),
    );
    let snapshot = db.snapshot().unwrap();
    let edit = snapshot
        .rename_at(&uri, syl_span::SourcePosition::new(0, 7), "answer")
        .unwrap()
        .unwrap();
    let mapped = workspace_edit(edit).unwrap();
    let changes = mapped.changes.unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[&lsp::Url::parse(uri.as_str()).unwrap()].len(), 2);
    let error = snapshot
        .rename_at(&uri, syl_span::SourcePosition::new(0, 7), "cell")
        .unwrap_err();
    assert_eq!(query_error(error).code, ErrorCode::InvalidParams);
    assert_eq!(
        query_error(query::QueryError::Cancelled).code,
        ErrorCode::RequestCancelled
    );
}
