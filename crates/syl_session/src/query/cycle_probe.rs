//! A tracked query that calls itself.
//!
//! Language recursion is not this. Const, enum, and import cycles are handled
//! inside semantic analysis. A query-implementation cycle has no `cycle_fn`,
//! so Salsa reports it instead of turning it into a Syl diagnostic.

use super::{RevisionInput, SylDb};

#[salsa::tracked(returns(copy))]
fn query_cycle_probe(db: &dyn SylDb, revision: RevisionInput) -> u32 {
    let _ = revision.files(db);
    query_cycle_probe(db, revision)
}

pub(crate) fn run(db: &dyn SylDb, revision: RevisionInput) {
    let _ = query_cycle_probe(db, revision);
}
