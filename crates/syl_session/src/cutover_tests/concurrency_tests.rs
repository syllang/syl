//! Two revisions of one file run a tracked query at the same time.
//! One case checks the AST span of a finished elaboration. The other
//! checks a semantic diagnostic span. Dropping one snapshot does not
//! cancel the other revision's query.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::query::QueryGate;
use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectConfig, ProjectError};

use super::TempTree;

#[test]
fn overlapping_revisions_read_their_own_text_and_spans() {
    let (older, newer, _host, _root) = two_revisions();
    let (older_seen, newer_seen) = pause_both(older, newer, elaborate_and_observe);

    assert!(
        older_seen.text.contains("y := 1"),
        "S0 input mixed: {}",
        older_seen.text
    );
    assert!(
        older_seen.map_text.contains("y := 1"),
        "S0 source map mixed: {}",
        older_seen.map_text
    );
    assert!(
        newer_seen.text.contains("y := 0"),
        "S1 input mixed: {}",
        newer_seen.text
    );
    assert!(
        newer_seen.map_text.contains("y := 0"),
        "S1 source map mixed: {}",
        newer_seen.map_text
    );
    assert_eq!(older_seen.semantic_diagnostics, 0);
    assert_eq!(newer_seen.semantic_diagnostics, 0);
    assert_eq!(older_seen.ast_span.text, "1");
    assert_eq!(newer_seen.ast_span.text, "0");
    assert_ne!(older_seen.text, newer_seen.text);
}

#[test]
fn overlapping_revisions_keep_their_diagnostic_spans() {
    let (older, newer, _host, _root) = revisions(
        "cell Top(y: out Bit) { y := old_name }\n",
        "cell Top(y: out Bit) { y := new_name }\n",
    );
    let (older_seen, newer_seen) = pause_both(older, newer, diagnose_and_observe);

    assert!(
        older_seen.text.contains("y := old_name"),
        "S0 input mixed: {}",
        older_seen.text
    );
    assert!(
        newer_seen.text.contains("y := new_name"),
        "S1 input mixed: {}",
        newer_seen.text
    );
    assert_eq!(older_seen.ast_span.text, "old_name");
    assert_eq!(newer_seen.ast_span.text, "new_name");
    assert_eq!(older_seen.diagnostic_span.text, "old_name");
    assert_eq!(newer_seen.diagnostic_span.text, "new_name");
    assert_ne!(
        older_seen.diagnostic_span.text,
        newer_seen.diagnostic_span.text
    );
}

#[test]
fn dropping_the_old_snapshot_does_not_cancel_the_new_revision() {
    let (older, newer, mut host, _root) = two_revisions();
    let gate = QueryGate::new();
    let _release = ReleaseOnDrop(Arc::clone(&gate));
    let newer_gate = Arc::clone(&gate);
    let worker = thread::spawn(move || {
        newer_gate.arm();
        module_count(&newer)
    });
    gate.entered();
    drop(older);
    gate.release();
    let modules = worker.join().expect("newer revision").expect("query");
    assert!(modules >= 1, "dropping S0 must not cancel S1");
    let again = host.snapshot().expect("host still publishes");
    let uri = again.files()[0].uri().clone();
    assert!(
        again.input_text(uri.as_str()).unwrap().contains("y := 0"),
        "the host must still publish the newer text"
    );
}

#[test]
fn dropping_the_new_snapshot_does_not_cancel_the_old_revision() {
    let (older, newer, mut host, _root) = two_revisions();
    let gate = QueryGate::new();
    let _release = ReleaseOnDrop(Arc::clone(&gate));
    let older_gate = Arc::clone(&gate);
    let worker = thread::spawn(move || -> Result<(String, usize), ProjectError> {
        older_gate.arm();
        let seen = revision_text(&older);
        let modules = module_count(&older)?;
        Ok((seen, modules))
    });
    gate.entered();
    drop(newer);
    gate.release();
    let (text, modules) = worker
        .join()
        .expect("older revision")
        .expect("frozen query");
    assert!(
        text.contains("y := 1"),
        "dropping S1 must not change S0: {text}"
    );
    assert!(modules >= 1, "dropping S1 must not cancel S0");
    let again = host.snapshot().expect("host still publishes");
    let uri = again.files()[0].uri().clone();
    assert!(again.input_text(uri.as_str()).unwrap().contains("y := 0"));
}

struct Seen {
    text: String,
    map_text: String,
    semantic_diagnostics: usize,
    ast_span: OwnedSpan,
}

struct DiagSeen {
    text: String,
    ast_span: OwnedSpan,
    diagnostic_span: OwnedSpan,
}

struct OwnedSpan {
    text: String,
}

fn pause_both<T: Send + 'static>(
    older: AnalysisSnapshot,
    newer: AnalysisSnapshot,
    observe: fn(&AnalysisSnapshot) -> T,
) -> (T, T) {
    let older_gate = QueryGate::new();
    let newer_gate = QueryGate::new();
    let _release_older = ReleaseOnDrop(Arc::clone(&older_gate));
    let _release_newer = ReleaseOnDrop(Arc::clone(&newer_gate));
    let older_done = Arc::new(AtomicBool::new(false));
    let newer_done = Arc::new(AtomicBool::new(false));
    let older_armed = Arc::clone(&older_gate);
    let newer_armed = Arc::clone(&newer_gate);
    let older_flag = Arc::clone(&older_done);
    let newer_flag = Arc::clone(&newer_done);
    let older_thread = thread::spawn(move || {
        older_armed.arm();
        let seen = observe(&older);
        older_flag.store(true, Ordering::Release);
        seen
    });
    let newer_thread = thread::spawn(move || {
        newer_armed.arm();
        let seen = observe(&newer);
        newer_flag.store(true, Ordering::Release);
        seen
    });

    older_gate.entered();
    newer_gate.entered();
    assert!(
        !older_done.load(Ordering::Acquire),
        "S0 left workspace_elaboration before S1 entered it"
    );
    assert!(
        !newer_done.load(Ordering::Acquire),
        "S1 left workspace_elaboration before both queries were inside it"
    );
    older_gate.release();
    newer_gate.release();

    let older_seen = older_thread.join().expect("older revision");
    let newer_seen = newer_thread.join().expect("newer revision");
    (older_seen, newer_seen)
}

fn elaborate_and_observe(snapshot: &AnalysisSnapshot) -> Seen {
    let modules = module_count(snapshot).expect("workspace_elaboration");
    assert!(modules >= 1, "the tracked elaboration query must finish");
    let (text, map_text, file_source, ast) = revision_file(snapshot);
    let ast_span = owned_span(snapshot, file_source, assignment_value_span(&ast));
    Seen {
        text,
        map_text,
        semantic_diagnostics: snapshot.semantic_diagnostics().len(),
        ast_span,
    }
}

fn diagnose_and_observe(snapshot: &AnalysisSnapshot) -> DiagSeen {
    let modules = module_count(snapshot).expect("workspace_elaboration");
    assert_eq!(
        modules, 0,
        "an unresolved assignment emits no hardware module"
    );
    let (text, _map_text, file_source, ast) = revision_file(snapshot);
    let marker = if text.contains("old_name") {
        "old_name"
    } else {
        "new_name"
    };
    DiagSeen {
        text,
        ast_span: owned_span(snapshot, file_source, assignment_value_span(&ast)),
        diagnostic_span: owned_span(snapshot, file_source, semantic_span_for(snapshot, marker)),
    }
}

fn revision_file(
    snapshot: &AnalysisSnapshot,
) -> (String, String, syl_span::SourceId, syl_syntax::AstFile) {
    let uri = snapshot.files()[0].uri().clone();
    let text = snapshot
        .input_text(uri.as_str())
        .expect("input text")
        .to_string();
    let file = snapshot.file_by_uri(&uri).expect("file");
    let map_text = snapshot
        .source_map()
        .file(file.source_id())
        .expect("span file")
        .text()
        .to_string();
    (text, map_text, file.source_id(), file.ast().clone())
}

fn owned_span(
    snapshot: &AnalysisSnapshot,
    file_source: syl_span::SourceId,
    span: syl_span::Span,
) -> OwnedSpan {
    assert_eq!(
        span.source, file_source,
        "span SourceId does not match this revision's file"
    );
    let mapped = snapshot
        .source_map()
        .file(span.source)
        .expect("span SourceId is missing from this revision's source map");
    assert_eq!(mapped.id(), span.source);
    assert!(
        span.start < span.end && span.end <= mapped.text().len(),
        "span range {}..{} is outside this revision",
        span.start,
        span.end
    );
    OwnedSpan {
        text: mapped.text()[span.start..span.end].to_string(),
    }
}

fn assignment_value_span(ast: &syl_syntax::AstFile) -> syl_span::Span {
    let cell = ast
        .items
        .iter()
        .find_map(|item| match item {
            syl_syntax::Item::Cell(cell) => Some(cell),
            _ => None,
        })
        .expect("cell");
    let value = cell
        .body
        .stmts
        .iter()
        .find_map(|stmt| match stmt {
            syl_syntax::Stmt::Assign { value, .. } | syl_syntax::Stmt::Drive { value, .. } => {
                Some(value)
            }
            _ => None,
        })
        .expect("assignment");
    match value {
        syl_syntax::Expr::Int(_, span) | syl_syntax::Expr::Ident(_, span) => *span,
        other => panic!("assignment value has no span: {other:?}"),
    }
}

fn semantic_span_for(snapshot: &AnalysisSnapshot, marker: &str) -> syl_span::Span {
    let diagnostics = snapshot.semantic_diagnostics();
    diagnostics
        .iter()
        .find(|diagnostic| {
            snapshot
                .source_map()
                .file(diagnostic.span.source)
                .and_then(|file| file.text().get(diagnostic.span.start..diagnostic.span.end))
                == Some(marker)
        })
        .unwrap_or_else(|| panic!("missing diagnostic span for {marker}: {diagnostics:?}"))
        .span
}

fn revision_text(snapshot: &AnalysisSnapshot) -> String {
    let uri = snapshot.files()[0].uri().clone();
    snapshot
        .input_text(uri.as_str())
        .expect("input text")
        .to_string()
}

fn module_count(snapshot: &AnalysisSnapshot) -> Result<usize, ProjectError> {
    let design = snapshot.hwir_with_token(&CancellationToken::new())?;
    Ok(design.map(|design| design.modules().len()).unwrap_or(0))
}

fn two_revisions() -> (AnalysisSnapshot, AnalysisSnapshot, AnalysisHost, TempTree) {
    revisions(
        "cell Top(y: out Bit) { y := 1 }\n",
        "cell Top(y: out Bit) { y := 0 }\n",
    )
}

fn revisions(
    older_source: &str,
    newer_source: &str,
) -> (AnalysisSnapshot, AnalysisSnapshot, AnalysisHost, TempTree) {
    let root = TempTree::new();
    let path = root.path.join("top.syl");
    std::fs::write(&path, older_source).unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![path.clone()]);
    let older = host.snapshot().expect("S0");
    std::fs::write(&path, newer_source).unwrap();
    let newer = host.snapshot().expect("S1");
    (older, newer, host, root)
}

struct ReleaseOnDrop(Arc<QueryGate>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}
