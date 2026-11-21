//! Test-only pause inside one tracked query.
//!
//! The gate is armed on the thread that should block. Other threads keep
//! running, so a waiter can block on the same Salsa key while this thread
//! still holds the claim.

use std::cell::RefCell;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

thread_local! {
    static ARMED: RefCell<Option<Arc<QueryGate>>> = const { RefCell::new(None) };
}

#[derive(Debug)]
struct GateState {
    entered: bool,
    release: bool,
}

/// One-shot pause for the thread that calls [`QueryGate::arm`].
#[derive(Debug)]
pub(crate) struct QueryGate {
    state: Mutex<GateState>,
    cv: Condvar,
}

impl QueryGate {
    pub(crate) fn new() -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(GateState {
                entered: false,
                release: false,
            }),
            cv: Condvar::new(),
        })
    }

    /// The next tracked pause on this thread waits until [`QueryGate::release`].
    pub(crate) fn arm(self: &Arc<Self>) {
        ARMED.with(|slot| *slot.borrow_mut() = Some(Arc::clone(self)));
    }

    pub(crate) fn entered(&self) {
        let mut state = lock_state(&self.state);
        let deadline = Instant::now() + Duration::from_secs(10);
        while !state.entered {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                drop(state);
                panic!("query gate was not entered");
            }
            let (next, timeout) = self
                .cv
                .wait_timeout(state, remaining)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            state = next;
            if timeout.timed_out() && !state.entered {
                drop(state);
                panic!("query gate was not entered");
            }
        }
    }

    pub(crate) fn release(&self) {
        let mut state = lock_state(&self.state);
        state.release = true;
        self.cv.notify_all();
    }
}

fn lock_state(state: &Mutex<GateState>) -> std::sync::MutexGuard<'_, GateState> {
    state
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

pub(super) fn wait_if_armed() {
    let gate = ARMED.with(|slot| slot.borrow_mut().take());
    let Some(gate) = gate else {
        return;
    };
    let mut state = lock_state(&gate.state);
    state.entered = true;
    gate.cv.notify_all();
    let deadline = Instant::now() + Duration::from_secs(30);
    while !state.release {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            drop(state);
            panic!("query gate was not released");
        }
        let (next, timeout) = gate
            .cv
            .wait_timeout(state, remaining)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        state = next;
        if timeout.timed_out() && !state.release {
            drop(state);
            panic!("query gate was not released");
        }
    }
}
