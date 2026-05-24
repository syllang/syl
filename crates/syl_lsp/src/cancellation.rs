use std::{collections::BTreeMap, sync::Mutex};
use syl_session::CancellationToken;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(crate) enum CancellationSlot {
    Diagnostics,
    Hover,
    Definition,
    Completion,
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub(crate) struct CancellationRegistry {
    active: Mutex<BTreeMap<CancellationSlot, CancellationToken>>,
}

impl CancellationRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn replace(&self, slot: CancellationSlot) -> CancellationToken {
        let token = CancellationToken::new();
        let mut active = self.active.lock().expect(
            "cancellation registry poisoning means request supersession tracking is unusable",
        );
        if let Some(previous) = active.insert(slot, token.clone()) {
            previous.cancel();
        }
        token
    }
}
