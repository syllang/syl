//! Same-key cancellation. One thread owns the query claim; the other waits on it.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;

use crate::query::QueryGate;
use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectConfig, ProjectError};

use super::TempTree;

#[test]
fn cancelled_owner_does_not_store_while_same_key_waiter_finishes() {
    let root = TempTree::new();
    let path = root.path.join("top.syl");
    std::fs::write(&path, "cell Top(y: out Bit) { y := 1 }\n").unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![path]);
    let snapshot = host.snapshot().expect("cell snapshot");
    assert!(
        snapshot.diagnostics().is_empty(),
        "the fixture must reach elaboration: {:?}",
        snapshot.diagnostics()
    );

    let gate = QueryGate::new();
    let _release = ReleaseOnDrop(Arc::clone(&gate));
    let owner_token = CancellationToken::new();
    let owner_snapshot = snapshot.clone();
    let owner_gate = Arc::clone(&gate);
    let owner_token_for_thread = owner_token.clone();
    let owner = thread::spawn(move || {
        owner_gate.arm();
        module_count(&owner_snapshot, &owner_token_for_thread)
    });
    gate.entered();
    assert_eq!(
        snapshot.execution_count("workspace_elaboration("),
        1,
        "the owner is inside the shared elaboration query"
    );

    let waiter_snapshot = snapshot.clone();
    let started = Arc::new(AtomicBool::new(false));
    let finished = Arc::new(AtomicBool::new(false));
    let started_flag = Arc::clone(&started);
    let finished_flag = Arc::clone(&finished);
    let waiter = thread::spawn(move || {
        started_flag.store(true, Ordering::Release);
        let result = module_count(&waiter_snapshot, &CancellationToken::new());
        finished_flag.store(true, Ordering::Release);
        result
    });
    while !started.load(Ordering::Acquire) {
        thread::yield_now();
    }
    thread::sleep(Duration::from_millis(200));
    assert!(
        !finished.load(Ordering::Acquire),
        "the waiter returned while the owner still held the elaboration claim"
    );
    assert_eq!(
        snapshot.execution_count("workspace_elaboration("),
        1,
        "the waiter must be blocked on the owner's claim, not running a second copy"
    );

    owner_token.cancel();
    gate.release();
    let owner_result = owner.join().expect("owner thread");
    assert!(
        matches!(owner_result, Err(ProjectError::Cancelled)),
        "the cancelled owner must not observe a stored result: {owner_result:?}"
    );
    let waiter_modules = waiter.join().expect("waiter thread").expect("waiter");
    assert!(
        waiter_modules >= 1,
        "the waiter must receive the complete hardware design"
    );
    let completed = snapshot.execution_count("workspace_elaboration(");
    assert!(
        completed >= 2,
        "cancelling the owner must not leave a successful memo for the waiter"
    );

    let memo_modules = module_count(&snapshot, &CancellationToken::new()).expect("retry");
    assert_eq!(memo_modules, waiter_modules);
    assert_eq!(
        snapshot.execution_count("workspace_elaboration("),
        completed,
        "a later request must reuse the complete memo"
    );
}

fn module_count(
    snapshot: &AnalysisSnapshot,
    token: &CancellationToken,
) -> Result<usize, ProjectError> {
    let design = snapshot.hwir_with_token(token)?;
    Ok(design.map(|design| design.modules().len()).unwrap_or(0))
}

struct ReleaseOnDrop(Arc<QueryGate>);

impl Drop for ReleaseOnDrop {
    fn drop(&mut self) {
        self.0.release();
    }
}
