//! Two revisions of one file run a tracked query at the same time, and
//! dropping one snapshot does not cancel the other revision's query.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;

use crate::query::QueryGate;
use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectConfig, ProjectError};

use super::TempTree;

#[test]
fn overlapping_revisions_read_their_own_text_and_spans() {
    let (older, newer, _host, _root) = two_revisions();
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
        let seen = elaborate_and_observe(&older);
        older_flag.store(true, Ordering::Release);
        seen
    });
    let newer_thread = thread::spawn(move || {
        newer_armed.arm();
        let seen = elaborate_and_observe(&newer);
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

fn elaborate_and_observe(snapshot: &AnalysisSnapshot) -> Seen {
    let modules = module_count(snapshot).expect("workspace_elaboration");
    assert!(modules >= 1, "the tracked elaboration query must finish");
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
    let _ast = file.ast();
    let diagnostics = snapshot.semantic_diagnostics().len();
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
