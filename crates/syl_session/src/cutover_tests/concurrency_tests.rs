//! Two revisions of one file answer at the same time, and dropping one
//! snapshot does not cancel the other revision's query.

use std::sync::{Arc, Barrier};
use std::thread;

use crate::query::QueryGate;
use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectConfig, ProjectError};

use super::TempTree;

#[test]
fn overlapping_revisions_read_their_own_text_and_spans() {
    let (older, newer, _host, _root) = two_revisions();
    let start = Arc::new(Barrier::new(2));
    let end = Arc::new(Barrier::new(2));
    let older_start = Arc::clone(&start);
    let older_end = Arc::clone(&end);
    let newer_start = Arc::clone(&start);
    let newer_end = Arc::clone(&end);
    let older_thread = thread::spawn(move || observe(&older, &older_start, &older_end));
    let newer_thread = thread::spawn(move || observe(&newer, &newer_start, &newer_end));
    let older_seen = older_thread.join().expect("older revision");
    let newer_seen = newer_thread.join().expect("newer revision");

    assert!(
        older_seen.text.contains("y := 1"),
        "S0 input text mixed with S1: {}",
        older_seen.text
    );
    assert!(
        older_seen.map_text.contains("y := 1"),
        "S0 source map mixed with S1: {}",
        older_seen.map_text
    );
    assert!(
        newer_seen.text.contains("y := 0"),
        "S1 input text mixed with S0: {}",
        newer_seen.text
    );
    assert!(
        newer_seen.map_text.contains("y := 0"),
        "S1 source map mixed with S0: {}",
        newer_seen.map_text
    );
    assert_eq!(older_seen.diagnostics, 0);
    assert_eq!(newer_seen.diagnostics, 0);
    assert_ne!(older_seen.text, newer_seen.text);
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
    diagnostics: usize,
}

fn observe(snapshot: &AnalysisSnapshot, start: &Barrier, end: &Barrier) -> Seen {
    start.wait();
    let uri = snapshot.files()[0].uri().clone();
    let text = snapshot
        .input_text(uri.as_str())
        .expect("input text")
        .to_string();
    let source_id = snapshot.file_by_uri(&uri).expect("file").source_id();
    let map_text = snapshot
        .source_map()
        .file(source_id)
        .expect("span file")
        .text()
        .to_string();
    let _ast = snapshot.file_by_uri(&uri).expect("ast").ast();
    let diagnostics = snapshot.semantic_diagnostics().len();
    end.wait();
    Seen {
        text,
        map_text,
        diagnostics,
    }
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
    let root = TempTree::new();
    let path = root.path.join("top.syl");
    std::fs::write(&path, "cell Top(y: out Bit) { y := 1 }\n").unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![path.clone()]);
    let older = host.snapshot().expect("S0");
    std::fs::write(&path, "cell Top(y: out Bit) { y := 0 }\n").unwrap();
    let newer = host.snapshot().expect("S1");
    (older, newer, host, root)
}

struct ReleaseOnDrop(Arc<QueryGate>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}
