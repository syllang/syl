//! Per-module Const MIR, Map IR, and EIR. Driver facts and DRC stay one design query.
//!
//! A module query reads that owner's typechecked arena plus the full bodies of
//! functions, maps, and `inplace` cells it actually expands. A `place`d cell
//! is elaborated on its own. An opaque overlay is read only when driver facts
//! and DRC run, so it does not recompute EIR.

use std::sync::Arc;

use syl_elab::{ConstMirStage, EirBuildStage, ElaborationOutput, HardwareCompiler, MapIrStage};
use syl_sema::{StageOutput, SymbolKind, TirAnalysis};
use syl_span::{Diagnostic, Span};

use super::{RevisionInput, SourceFileInput, SylDb, SymbolId};
use crate::query::lower;

pub(super) fn design_output(
    db: &dyn SylDb,
    revision: RevisionInput,
    files: &[SourceFileInput],
) -> Option<Arc<ElaborationOutput>> {
    lower::record_hardware(db, revision, files);
    let hardware = hardware_symbols(db, files);
    let mut stored = Vec::new();
    let mut diagnostics = Vec::new();
    for symbol in hardware {
        super::unwind_if_request_cancelled(db);
        let module = owner_module_eir(db, revision, symbol);
        match module.as_ref().as_ref() {
            Ok(_) => stored.push(module),
            Err(diagnostic) => diagnostics.push(diagnostic.clone()),
        }
    }
    super::unwind_if_request_cancelled(db);
    let compiler = HardwareCompiler::with_opaque_summaries(revision.opaque(db).clone());
    let output = if diagnostics.is_empty() {
        let modules = stored
            .iter()
            .filter_map(|module| module.as_ref().as_ref().ok())
            .collect::<Vec<_>>();
        compiler.output_from_modules(&modules, &super::request_cancelled)
    } else {
        compiler.diagnostics_output(diagnostics)
    };
    super::unwind_if_request_cancelled(db);
    Some(Arc::new(output))
}

fn hardware_symbols<'db>(db: &'db dyn SylDb, files: &[SourceFileInput]) -> Vec<SymbolId<'db>> {
    let mut symbols = Vec::new();
    for file in files {
        let index = super::file_item_index(db, *file);
        for key in &index.keys {
            if matches!(key.kind, SymbolKind::Cell | SymbolKind::ExternCell) {
                symbols.push(super::intern_symbol(db, key));
            }
        }
    }
    symbols
}

#[salsa::tracked(returns(ref), no_eq)]
fn elab_tir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> Arc<StageOutput<TirAnalysis>> {
    super::observe_cancel(db);
    let Some(file) = lower::file_for_module(db, revision, symbol.module(db)) else {
        return Arc::clone(super::owner_tir(db, revision, symbol));
    };
    let item = super::owner_item(db, file, symbol);
    let companions = lower::elab_companions(db, revision, file, symbol, &item.0);
    if companions.is_empty() {
        return Arc::clone(super::owner_tir(db, revision, symbol));
    }
    let mut owners = Vec::new();
    for companion in companions {
        let key = lower::key_from_symbol(db, companion);
        let tir = Arc::clone(super::owner_tir(db, revision, companion));
        owners.push((key, tir));
    }
    let owner_key = lower::key_from_symbol(db, symbol);
    let owner = Arc::clone(super::owner_tir(db, revision, symbol));
    owners.push((owner_key, owner));
    let refs = owners
        .iter()
        .map(|(key, tir)| (key.clone(), tir.as_ref()))
        .collect::<Vec<_>>();
    Arc::new(syl_sema::aggregate_owner_tir(&refs))
}

#[salsa::tracked(returns(ref), no_eq)]
fn owner_const_mir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> Arc<Result<ConstMirStage, Diagnostic>> {
    super::observe_cancel(db);
    stage_result(elab_tir(db, revision, symbol), |analysis| {
        HardwareCompiler::new().const_mir_stage(analysis)
    })
}

#[salsa::tracked(returns(ref), no_eq)]
fn owner_map_ir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> Arc<Result<MapIrStage, Diagnostic>> {
    super::observe_cancel(db);
    stage_result(elab_tir(db, revision, symbol), |analysis| {
        HardwareCompiler::new().map_ir_stage(analysis)
    })
}

#[salsa::tracked(returns(ref), no_eq)]
fn owner_module_eir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> Arc<Result<EirBuildStage, Diagnostic>> {
    super::observe_cancel(db);
    let tir = elab_tir(db, revision, symbol);
    let Some(analysis) = tir.partial_stage() else {
        return Arc::new(Err(missing_tir()));
    };
    let const_mir = owner_const_mir(db, revision, symbol);
    let Ok(const_mir) = const_mir.as_ref().as_ref() else {
        return Arc::new(Err(const_mir.as_ref().as_ref().unwrap_err().clone()));
    };
    let map_ir = owner_map_ir(db, revision, symbol);
    let Ok(map_ir) = map_ir.as_ref().as_ref() else {
        return Arc::new(Err(map_ir.as_ref().as_ref().unwrap_err().clone()));
    };
    let name = symbol.name(db).clone();
    Arc::new(
        HardwareCompiler::new()
            .module_eir_stage(analysis, const_mir, map_ir, &name)
            .map_err(Diagnostic::from),
    )
}

fn stage_result<T>(
    tir: &StageOutput<TirAnalysis>,
    build: impl FnOnce(&TirAnalysis) -> Result<T, syl_elab::CompileError>,
) -> Arc<Result<T, Diagnostic>> {
    let Some(analysis) = tir.partial_stage() else {
        return Arc::new(Err(missing_tir()));
    };
    Arc::new(build(analysis).map_err(Diagnostic::from))
}

fn missing_tir() -> Diagnostic {
    Diagnostic::new(Span::default(), "hardware module has no typed IR")
}
