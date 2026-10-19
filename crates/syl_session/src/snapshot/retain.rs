//! Keeps Salsa's `Arc` results alive so a snapshot can return references after
//! the database lock is released. This is a borrow adapter, not a second memo.

use std::cell::RefCell;
use std::sync::Arc;

use syl_elab::ElaborationOutput;
use syl_sema::{HirAnalysisOutput, OpaqueSummaryTable, StageOutput, TirAnalysis};

#[derive(Clone, Default)]
pub(super) struct Retained {
    hir: Vec<Arc<HirAnalysisOutput>>,
    tir: Vec<Arc<StageOutput<TirAnalysis>>>,
    elaboration: Vec<Arc<ElaborationOutput>>,
    opaque: Vec<Arc<OpaqueSummaryTable>>,
}

pub(super) fn keep_hir(
    kept: &RefCell<Retained>,
    value: Arc<HirAnalysisOutput>,
) -> &HirAnalysisOutput {
    retain(kept, value, |slot| &mut slot.hir)
}

pub(super) fn keep_tir(
    kept: &RefCell<Retained>,
    value: Arc<StageOutput<TirAnalysis>>,
) -> &StageOutput<TirAnalysis> {
    retain(kept, value, |slot| &mut slot.tir)
}

pub(super) fn keep_elaboration(
    kept: &RefCell<Retained>,
    value: Arc<ElaborationOutput>,
) -> &ElaborationOutput {
    retain(kept, value, |slot| &mut slot.elaboration)
}

pub(super) fn keep_opaque(
    kept: &RefCell<Retained>,
    value: OpaqueSummaryTable,
) -> &OpaqueSummaryTable {
    retain(kept, Arc::new(value), |slot| &mut slot.opaque)
}

fn retain<T>(
    kept: &RefCell<Retained>,
    value: Arc<T>,
    slot: impl FnOnce(&mut Retained) -> &mut Vec<Arc<T>>,
) -> &T {
    let ptr = Arc::as_ptr(&value);
    slot(&mut kept.borrow_mut()).push(value);
    // SAFETY: `ptr` addresses the Arc allocation. Moving the Arc into the
    // append-only slot does not move that allocation, and entries stay
    // reachable for the returned borrow.
    unsafe { &*ptr }
}
