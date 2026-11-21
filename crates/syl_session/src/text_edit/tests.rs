use super::*;
use crate::{AnalysisHost, DocumentVersion};

fn edit(a: usize, b: usize, c: usize, d: usize, text: &str) -> TextDocumentEdit {
    TextDocumentEdit::incremental(
        SourceRange::new(SourcePosition::new(a, b), SourcePosition::new(c, d)),
        text,
    )
}

#[test]
fn sequential_edits_join_crlf_and_round_surrogates_down() {
    let uri = DocumentUri::new("file:///test.syl");
    let edits = [
        TextDocumentEdit::full("a💡\r\nb"),
        edit(0, 99, 1, 0, ""),
        edit(0, 2, 0, 3, "!"),
    ];
    assert_eq!(apply(&uri, "", &edits).unwrap(), "a!b");
}

#[test]
fn insert_delete_replace_and_multiline_coordinates_are_sequential() {
    let uri = DocumentUri::new("untitled:syl/edits");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), "abc\ndef\n".into(), DocumentVersion::new(1));
    let edits = [
        edit(0, 1, 0, 1, "💡"),
        edit(0, 3, 0, 4, ""),
        edit(0, 3, 0, 4, "B\r\nC"),
        edit(1, 1, 2, 0, ""),
    ];
    assert_eq!(
        host.apply_document_changes_at_version(&uri, &edits, DocumentVersion::new(2))
            .unwrap(),
        DocumentVersion::new(2)
    );
    assert_eq!(host.overlay(&uri).unwrap().text(), "a💡B\r\nCdef\n");
    host.apply_document_changes_at_version(
        &uri,
        &[edit(99, 99, 100, 0, "tail")],
        DocumentVersion::new(3),
    )
    .unwrap();
    assert!(host.overlay(&uri).unwrap().text().ends_with("\ntail"));
}

#[test]
fn line_end_clamping_preserves_crlf_and_empty_lines() {
    let uri = DocumentUri::new("untitled:syl/crlf");
    assert_eq!(
        apply(&uri, "ab\r\n\r\nc", &[edit(0, 99, 0, 99, "!")]).unwrap(),
        "ab!\r\n\r\nc"
    );
    assert_eq!(
        apply(&uri, "ab\nc", &[edit(0, 2, 1, 0, "")]).unwrap(),
        "abc"
    );
    assert_eq!(apply(&uri, "\r\n", &[edit(0, 0, 1, 0, "")]).unwrap(), "");
    assert_eq!(apply(&uri, "", &[edit(0, 0, 0, 0, "x")]).unwrap(), "x");
}

#[test]
fn invalid_batch_and_stale_versions_leave_buffer_version_and_revision_unchanged() {
    let uri = DocumentUri::new("untitled:syl/atomic");
    let mut host = AnalysisHost::new();
    assert!(matches!(
        host.apply_document_changes_at_version(&uri, &[], DocumentVersion::new(2)),
        Err(ProjectError::DocumentNotOpen { .. })
    ));
    host.open_document(uri.clone(), "abc".into(), DocumentVersion::new(3));
    let revision = host.database().revision();
    let invalid = [TextDocumentEdit::full("changed"), edit(0, 4, 0, 1, "")];
    assert!(matches!(
        host.apply_document_changes_at_version(&uri, &invalid, DocumentVersion::new(4)),
        Err(ProjectError::InvalidTextEdit { .. })
    ));
    assert!(matches!(
        host.apply_document_changes_at_version(
            &uri,
            &[TextDocumentEdit::full("stale")],
            DocumentVersion::new(3)
        ),
        Err(ProjectError::StaleDocumentVersion { .. })
    ));
    assert_eq!(host.overlay(&uri).unwrap().text(), "abc");
    assert_eq!(
        host.overlay(&uri).unwrap().version(),
        DocumentVersion::new(3)
    );
    assert_eq!(host.database().revision(), revision);
    host.apply_document_changes_at_version(
        &uri,
        &[TextDocumentEdit::full("a"), edit(0, 1, 0, 1, "b")],
        DocumentVersion::new(4),
    )
    .unwrap();
    assert_eq!(host.overlay(&uri).unwrap().text(), "ab");
    assert_ne!(host.database().revision(), revision);
}
