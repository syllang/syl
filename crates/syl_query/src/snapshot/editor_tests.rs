use crate::EditorQueries;
use syl_session::{
    AnalysisDatabase, AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion,
};
use syl_span::{SourcePosition, SourceRange};

fn snapshot(text: &str) -> (AnalysisSnapshot, DocumentUri) {
    let mut db = AnalysisDatabase::new();
    let uri = DocumentUri::new("file:///editor_test.syl");
    db.open_document(uri.clone(), text.to_owned(), DocumentVersion::new(1));
    (db.snapshot().unwrap(), uri)
}

fn position(text: &str, needle: &str) -> SourcePosition {
    let offset = text.find(needle).unwrap();
    let prefix = &text[..offset];
    SourcePosition::new(
        prefix.bytes().filter(|b| *b == b'\n').count(),
        prefix.rsplit('\n').next().unwrap().encode_utf16().count(),
    )
}

#[test]
fn editor_references_rename_and_cancellation() {
    let text = "fn identity(x: nat) -> nat { let value = x; value }";
    let (s, uri) = snapshot(text);
    let p = position(text, "value =");
    let refs = s.references_at(&uri, p, true);
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert_eq!(s.references_at(&uri, p, false).len(), 1);
    let prepare = s.prepare_rename_at(&uri, p).unwrap();
    assert_eq!(prepare.placeholder, "value");
    let edit = s.rename_at(&uri, p, "answer").unwrap().unwrap();
    assert_eq!(edit.changes[&uri].len(), 2);
    for invalid in ["", "123abc", "cell", "UInt", "a b", " x", "x ", "this"] {
        assert!(s.rename_at(&uri, p, invalid).is_err(), "{invalid}");
    }
    assert!(s.rename_at(&uri, p, "x").is_err());
    let token = CancellationToken::new();
    token.cancel();
    assert!(s.references_at_with_token(&uri, p, true, &token).is_err());
}

#[test]
fn editor_symbols_hints_tokens_and_signature() {
    let text = "/// Identity\nfn identity(x: nat, y: nat) -> nat { let value = x; value }\nfn main() -> nat { identity(1, 2) }";
    let (s, uri) = snapshot(text);
    assert_eq!(s.workspace_symbols("idty")[0].name, "identity");
    let hints = s.inlay_hints(
        &uri,
        SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(10, 0)),
    );
    assert!(hints.iter().any(|h| h.label == ": nat"), "{hints:?}");
    let tokens = s.semantic_tokens(&uri);
    assert!(!tokens.is_empty());
    for pair in tokens.windows(2) {
        assert!(
            super::ident_span::range_key(pair[0].range)
                < super::ident_span::range_key(pair[1].range)
        );
    }
    let help = s.signature_help_at(&uri, position(text, "2)")).unwrap();
    assert_eq!(help.active_parameter, Some(1));
    assert_eq!(
        help.signatures[0].label,
        "fn identity(x: nat, y: nat) -> nat"
    );
    assert_eq!(
        help.signatures[0].documentation.as_deref(),
        Some("Identity")
    );
}

fn workspace(files: &[(&str, &str)]) -> (AnalysisSnapshot, Vec<DocumentUri>) {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let root = std::env::temp_dir().join(format!(
        "syl_editor_{}_{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&root).unwrap();
    let mut db = AnalysisDatabase::with_config(
        syl_session::ProjectConfig::new().with_workspace_root(root.clone()),
    );
    let paths: Vec<_> = files
        .iter()
        .map(|(name, text)| {
            let path = root.join(format!("{name}.syl"));
            std::fs::write(&path, text).unwrap();
            path
        })
        .collect();
    db.set_roots(paths.clone());
    let snapshot = db.snapshot().unwrap();
    let uris = paths
        .iter()
        .map(|path| {
            snapshot
                .files()
                .iter()
                .find(|f| f.path() == Some(path.as_path()))
                .unwrap()
                .uri()
                .clone()
        })
        .collect();
    std::fs::remove_dir_all(root).unwrap();
    (snapshot, uris)
}

fn source_at(s: &AnalysisSnapshot, location: &crate::LocationResult) -> String {
    let file = s.file_by_uri(&location.uri).unwrap();
    let source = s.source_map().file(file.source_id()).unwrap();
    let start = source.byte_offset_for_utf16_position(location.range.start);
    let end = source.byte_offset_for_utf16_position(location.range.end);
    source.text()[start..end].into()
}

#[path = "presentation_tests.rs"]
mod presentation_tests;
#[path = "references_tests.rs"]
mod references_tests;
#[path = "signature_help_tests.rs"]
mod signature_help_tests;
