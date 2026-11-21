//! Keeps Salsa's `Arc` results alive so a snapshot can return references after
//! the database lock is released. This is a borrow adapter, not a second memo.

use std::sync::{Arc, Mutex};

use syl_elab::ElaborationOutput;
use syl_sema::{HirAnalysisOutput, OpaqueSummaryTable, StageOutput, TirAnalysis};

#[derive(Default)]
pub(super) struct Retained {
    hir: Vec<Arc<HirAnalysisOutput>>,
    tir: Vec<Arc<StageOutput<TirAnalysis>>>,
    elaboration: Vec<Arc<ElaborationOutput>>,
    opaque: Vec<Arc<OpaqueSummaryTable>>,
}

impl Retained {
    #[cfg(test)]
    pub(super) fn counts(&self) -> (usize, usize, usize, usize) {
        (
            self.hir.len(),
            self.tir.len(),
            self.elaboration.len(),
            self.opaque.len(),
        )
    }
}

pub(super) fn keep_hir(
    kept: &Mutex<Retained>,
    value: Arc<HirAnalysisOutput>,
) -> &HirAnalysisOutput {
    retain(kept, value, |slot| &mut slot.hir)
}

pub(super) fn keep_tir(
    kept: &Mutex<Retained>,
    value: Arc<StageOutput<TirAnalysis>>,
) -> &StageOutput<TirAnalysis> {
    retain(kept, value, |slot| &mut slot.tir)
}

pub(super) fn keep_elaboration(
    kept: &Mutex<Retained>,
    value: Arc<ElaborationOutput>,
) -> &ElaborationOutput {
    retain(kept, value, |slot| &mut slot.elaboration)
}

pub(super) fn keep_opaque(
    kept: &Mutex<Retained>,
    value: Arc<OpaqueSummaryTable>,
) -> &OpaqueSummaryTable {
    retain(kept, value, |slot| &mut slot.opaque)
}

fn retain<T>(
    kept: &Mutex<Retained>,
    value: Arc<T>,
    slot: impl FnOnce(&mut Retained) -> &mut Vec<Arc<T>>,
) -> &T {
    let mut guard = kept
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entries = slot(&mut guard);
    if let Some(existing) = entries.iter().find(|entry| Arc::ptr_eq(entry, &value)) {
        let ptr = Arc::as_ptr(existing);
        // SAFETY: `ptr` addresses the Arc allocation held in the append-only
        // slot, and entries stay reachable for the returned borrow.
        return unsafe { &*ptr };
    }
    let ptr = Arc::as_ptr(&value);
    entries.push(value);
    // SAFETY: `ptr` addresses the Arc allocation. Moving the Arc into the
    // append-only slot does not move that allocation, and entries stay
    // reachable for the returned borrow.
    unsafe { &*ptr }
}
