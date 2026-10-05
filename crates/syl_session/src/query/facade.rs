//! Crate-visible wrappers around the tracked queries. Snapshot code calls these
//! names; the tracked functions stay private in the parent module.

use std::sync::Arc;

use syl_elab::ElaborationOutput;
use syl_sema::{HirAnalysisOutput, OpaqueSummaryTable, StageOutput, SymbolKey, TirAnalysis};

use super::{
    RevisionInput, SymbolId, UnitId, ide_unit_hir, ide_unit_tir, merged_opaque,
    package_diagnostics, workspace_diagnostics, workspace_elaboration, workspace_hir,
    workspace_tir,
};
use crate::query::SylDb;

pub(crate) fn intern_symbol<'db>(db: &'db dyn SylDb, key: &SymbolKey) -> SymbolId<'db> {
    SymbolId::new(
        db,
        key.module.join("."),
        key.kind.tag(),
        key.name.clone(),
        key.discriminator,
    )
}

pub(crate) fn workspace_hir_query(
    db: &dyn SylDb,
    revision: RevisionInput,
) -> &Arc<HirAnalysisOutput> {
    workspace_hir(db, revision)
}

pub(crate) fn workspace_tir_query(
    db: &dyn SylDb,
    revision: RevisionInput,
) -> &Arc<StageOutput<TirAnalysis>> {
    workspace_tir(db, revision)
}

pub(crate) fn workspace_elaboration_query(
    db: &dyn SylDb,
    revision: RevisionInput,
) -> &Option<Arc<ElaborationOutput>> {
    workspace_elaboration(db, revision)
}

pub(crate) fn workspace_diagnostics_query(
    db: &dyn SylDb,
    revision: RevisionInput,
) -> &[syl_span::Diagnostic] {
    workspace_diagnostics(db, revision)
}

pub(crate) fn ide_hir_query<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> &'db Arc<HirAnalysisOutput> {
    ide_unit_hir(db, revision, unit)
}

pub(crate) fn ide_tir_query<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> &'db Arc<StageOutput<TirAnalysis>> {
    ide_unit_tir(db, revision, unit)
}

pub(crate) fn package_diagnostics_query<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> &'db [syl_span::Diagnostic] {
    package_diagnostics(db, revision, unit)
}

pub(crate) fn merged_opaque_query(
    db: &dyn SylDb,
    revision: RevisionInput,
) -> &Option<OpaqueSummaryTable> {
    merged_opaque(db, revision)
}

pub(crate) fn make_unit<'db>(
    db: &'db dyn SylDb,
    name: &str,
    local_uris: Vec<String>,
    closure_uris: Vec<String>,
) -> UnitId<'db> {
    UnitId::new(db, name.to_string(), local_uris, closure_uris)
}

#[cfg(test)]
pub(crate) fn input_text(db: &dyn SylDb, revision: RevisionInput, uri: &str) -> Option<String> {
    revision
        .files(db)
        .iter()
        .copied()
        .find(|file| file.uri(db) == uri)
        .map(|file| file.text(db).clone())
}
