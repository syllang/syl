//! Private Salsa database. This module is the only memo and dependency owner.
//!
//! Arena ids never appear in query keys. `SymbolKey` is the cross-revision
//! identity; each HIR/TIR result builds a fresh arena for that result.

use crate::CancellationToken;
use crate::ProjectError;
use std::cell::RefCell;
use std::fmt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
#[cfg(test)]
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use syl_elab::ElaborationOutput;
use syl_sema::{
    DeclarationContract, HardwareContract, HirAnalysisOutput, OpaqueSummaryTable, SemanticSession,
    StageOutput, SymbolKey, SymbolKind, TirAnalysis, declaration_contracts, file_symbol_keys,
    item_has_storage,
};
use syl_span::SourceId;
use syl_syntax::parser::SourceParser;
use syl_syntax::{AstFile, ErrorItem, Item};

#[cfg(test)]
mod cycle_probe;
mod elab;
mod facade;
mod lower;
#[cfg(test)]
mod pause;
mod refs;
mod store;

#[cfg(test)]
pub(crate) use cycle_probe::run as probe_query_cycle;
#[cfg(test)]
pub(crate) use pause::QueryGate;

pub(crate) use store::{Generation, QueryStore};

#[cfg(test)]
pub(crate) use facade::input_text;
pub(crate) use facade::{
    ide_hir_query, ide_tir_query, intern_symbol, make_unit, merged_opaque_query,
    package_diagnostics_query, workspace_diagnostics_query, workspace_elaboration_query,
    workspace_hir_query, workspace_tir_query,
};

thread_local! {
    static REQUEST_CANCEL: RefCell<Option<CancellationToken>> = const { RefCell::new(None) };
}

pub(crate) fn catch_query<T>(
    token: &CancellationToken,
    body: impl FnOnce() -> T,
) -> Result<T, ProjectError> {
    let previous = REQUEST_CANCEL.with(|slot| slot.replace(Some(token.clone())));
    let result = salsa::Cancelled::catch(AssertUnwindSafe(body));
    REQUEST_CANCEL.with(|slot| *slot.borrow_mut() = previous);
    match result {
        Ok(value) => Ok(value),
        Err(_) => Err(ProjectError::Cancelled),
    }
}

fn request_cancelled() -> bool {
    REQUEST_CANCEL.with(|slot| {
        slot.borrow()
            .as_ref()
            .is_some_and(CancellationToken::is_cancelled)
    })
}

fn observe_cancel(db: &dyn SylDb) {
    if request_cancelled() {
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
    }
}

fn unwind_if_request_cancelled(db: &dyn SylDb) {
    if request_cancelled() {
        db.cancellation_token().cancel();
        db.unwind_if_revision_cancelled();
    }
}

/// Counts live snapshots. The host's own database handle is not included.
#[derive(Debug)]
pub(crate) struct LiveGuard {
    live: Arc<AtomicUsize>,
}

impl LiveGuard {
    pub(crate) fn enter(live: &Arc<AtomicUsize>) -> Self {
        live.fetch_add(1, Ordering::SeqCst);
        Self {
            live: Arc::clone(live),
        }
    }
}

impl Clone for LiveGuard {
    fn clone(&self) -> Self {
        self.live.fetch_add(1, Ordering::SeqCst);
        Self {
            live: Arc::clone(&self.live),
        }
    }
}

impl Drop for LiveGuard {
    fn drop(&mut self) {
        self.live.fetch_sub(1, Ordering::SeqCst);
    }
}

#[salsa::db]
pub(crate) trait SylDb: salsa::Database {}

#[salsa::db]
#[derive(Clone)]
pub(crate) struct SylDatabase {
    storage: salsa::Storage<Self>,
    #[cfg(test)]
    executions: Arc<Mutex<Vec<String>>>,
}

impl SylDatabase {
    pub(crate) fn new() -> Self {
        #[cfg(test)]
        let executions = Arc::new(Mutex::new(Vec::new()));
        let storage = {
            #[cfg(test)]
            {
                let log = Arc::clone(&executions);
                salsa::Storage::new(Some(Box::new(move |event| {
                    if let salsa::EventKind::WillExecute { database_key } = event.kind {
                        log.lock()
                            .expect("query execution log")
                            .push(format!("{database_key:?}"));
                    }
                })))
            }
            #[cfg(not(test))]
            {
                salsa::Storage::new(None)
            }
        };
        Self {
            storage,
            #[cfg(test)]
            executions,
        }
    }

    #[cfg(test)]
    pub(crate) fn execution_count(&self, needle: &str) -> usize {
        self.executions
            .lock()
            .expect("query execution log")
            .iter()
            .filter(|line| line.contains(needle))
            .count()
    }
}

impl fmt::Debug for SylDatabase {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SylDatabase")
            .finish_non_exhaustive()
    }
}

impl Default for SylDatabase {
    fn default() -> Self {
        Self::new()
    }
}

#[salsa::db]
impl salsa::Database for SylDatabase {}

#[salsa::db]
impl SylDb for SylDatabase {}

#[salsa::input]
pub(crate) struct SourceFileInput {
    #[returns(ref)]
    pub uri: String,
    #[returns(ref)]
    pub text: String,
    pub source_id: u32,
    #[returns(ref)]
    pub module_path: Vec<String>,
    pub version: u64,
}

#[salsa::input]
pub(crate) struct RevisionInput {
    #[returns(ref)]
    pub files: Vec<SourceFileInput>,
    #[returns(ref)]
    pub opaque: OpaqueSummaryTable,
}

#[salsa::interned]
pub(crate) struct SymbolId {
    #[returns(ref)]
    pub module: String,
    pub kind_tag: u8,
    #[returns(ref)]
    pub name: String,
    pub discriminator: u32,
}

#[salsa::interned]
pub(crate) struct UnitId {
    #[returns(ref)]
    pub name: String,
    #[returns(ref)]
    pub local_uris: Vec<String>,
    #[returns(ref)]
    pub closure_uris: Vec<String>,
}

#[derive(Clone, Debug)]
pub(crate) struct FileSpec {
    pub uri: String,
    pub text: String,
    pub source_id: u32,
    pub module_path: Vec<String>,
    pub version: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ContractList {
    contracts: Vec<DeclarationContract>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ItemIndex {
    keys: Vec<SymbolKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NamedSymbols {
    pub(super) hits: Vec<NamedHit>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct NamedHit {
    pub(super) key: SymbolKey,
    pub(super) index: u32,
}

#[salsa::interned]
pub(super) struct LookupName {
    #[returns(ref)]
    name: String,
}

#[derive(Clone, Debug, PartialEq)]
struct EqItem(Item);

impl Eq for EqItem {}

/// Stub item plus the signature text.
///
/// Equality includes the stub item. A length-changing edit before the
/// declaration moves definition spans, and those spans have to match the
/// snapshot source map. A body-only edit still compares equal when the stub
/// signature spans do not move. The fingerprint keeps a comment-only
/// signature text change unequal.
#[derive(Clone, Debug)]
pub(super) struct SignatureItem {
    pub(super) item: Item,
    pub(super) fingerprint: String,
}

impl PartialEq for SignatureItem {
    fn eq(&self, other: &Self) -> bool {
        self.fingerprint == other.fingerprint && self.item == other.item
    }
}

impl Eq for SignatureItem {}

impl SignatureItem {
    pub(super) fn new(item: Item, fingerprint: String) -> Self {
        Self { item, fingerprint }
    }
}

#[salsa::tracked(returns(ref), no_eq)]
fn parsed_file(db: &dyn SylDb, file: SourceFileInput) -> AstFile {
    observe_cancel(db);
    let source_id = SourceId::new(*file.source_id(db) as usize);
    SourceParser::new_in(file.text(db), source_id)
        .parse_file_partial()
        .file
}

#[salsa::tracked(returns(ref))]
fn file_item_index(db: &dyn SylDb, file: SourceFileInput) -> ItemIndex {
    observe_cancel(db);
    let ast = parsed_file(db, file);
    ItemIndex {
        keys: file_symbol_keys(file.module_path(db), ast),
    }
}

#[salsa::tracked(returns(ref))]
fn file_contracts(db: &dyn SylDb, file: SourceFileInput) -> ContractList {
    observe_cancel(db);
    let ast = parsed_file(db, file);
    let _ = file_item_index(db, file);
    ContractList {
        contracts: declaration_contracts(file.module_path(db), file.text(db), ast),
    }
}

/// Keys of one name. An edit to a different item leaves this value equal.
#[salsa::tracked(returns(ref))]
fn symbol_named<'db>(
    db: &'db dyn SylDb,
    file: SourceFileInput,
    name: LookupName<'db>,
) -> NamedSymbols {
    observe_cancel(db);
    let wanted = name.name(db);
    let hits = file_item_index(db, file)
        .keys
        .iter()
        .enumerate()
        .filter(|(_, key)| &key.name == wanted && !matches!(key.kind, SymbolKind::Error))
        .map(|(index, key)| NamedHit {
            key: key.clone(),
            index: index as u32,
        })
        .collect::<Vec<_>>();
    NamedSymbols { hits }
}

/// Source text of one item. A same-length edit elsewhere keeps this value equal.
#[salsa::tracked(returns(ref))]
fn owner_item_text<'db>(
    db: &'db dyn SylDb,
    file: SourceFileInput,
    symbol: SymbolId<'db>,
) -> String {
    observe_cancel(db);
    let span = owner_item(db, file, symbol).0.span();
    file.text(db)
        .get(span.start..span.end)
        .unwrap_or("")
        .to_string()
}

/// Module doc only. An unrelated edit leaves this value equal and backdates.
#[salsa::tracked(returns(ref))]
fn file_module_doc(db: &dyn SylDb, file: SourceFileInput) -> Option<String> {
    observe_cancel(db);
    parsed_file(db, file).doc.clone()
}

#[salsa::tracked(returns(ref))]
fn declaration_contract<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> DeclarationContract {
    observe_cancel(db);
    let module = symbol.module(db);
    let Some(file) = lower::file_for_module(db, revision, module) else {
        return lower::empty_contract(symbol, db);
    };
    let _ = file_contracts(db, file);
    file_contracts(db, file)
        .contracts
        .iter()
        .find(|contract| lower::same_symbol(db, symbol, &contract.key))
        .cloned()
        .unwrap_or_else(|| lower::empty_contract(symbol, db))
}

#[salsa::tracked(returns(ref))]
fn owner_item<'db>(db: &'db dyn SylDb, file: SourceFileInput, symbol: SymbolId<'db>) -> EqItem {
    observe_cancel(db);
    let ast = parsed_file(db, file);
    let keys = file_item_index(db, file).keys.clone();
    ast.items
        .iter()
        .zip(keys)
        .find(|(_, key)| lower::same_symbol(db, symbol, key))
        .map(|(item, _)| EqItem(item.clone()))
        .unwrap_or_else(|| EqItem(Item::Error(ErrorItem::new(syl_span::Span::default()))))
}

#[salsa::tracked(returns(ref))]
fn symbol_signature<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    file: SourceFileInput,
    symbol: SymbolId<'db>,
) -> SignatureItem {
    observe_cancel(db);
    lower::signature_item(db, revision, file, symbol)
}

#[salsa::tracked(returns(ref))]
fn hardware_contract<'db>(
    db: &'db dyn SylDb,
    file: SourceFileInput,
    symbol: SymbolId<'db>,
) -> HardwareContract {
    observe_cancel(db);
    let item = owner_item(db, file, symbol);
    HardwareContract::new(
        lower::key_from_symbol(db, symbol),
        item_has_storage(&item.0),
    )
}

#[salsa::tracked(returns(ref), no_eq)]
fn owner_hir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> Arc<HirAnalysisOutput> {
    observe_cancel(db);
    Arc::new(hir_for_owner(db, revision, symbol))
}

#[salsa::tracked(returns(ref), no_eq)]
fn owner_tir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbol: SymbolId<'db>,
) -> Arc<StageOutput<TirAnalysis>> {
    observe_cancel(db);
    Arc::new(owner_hir(db, revision, symbol).stage().check_tir_partial())
}

#[salsa::tracked(returns(ref), no_eq)]
fn ide_unit_hir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Arc<HirAnalysisOutput> {
    observe_cancel(db);
    let symbols = lower::local_symbols(db, revision, unit);
    lower::analysis_for_symbols(db, revision, &symbols)
}

#[salsa::tracked(returns(ref), no_eq)]
fn ide_unit_tir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Arc<StageOutput<TirAnalysis>> {
    observe_cancel(db);
    let symbols = lower::local_symbols(db, revision, unit);
    lower::tir_for_symbols(db, revision, &symbols)
}

#[salsa::tracked(returns(ref), no_eq)]
fn workspace_hir(db: &dyn SylDb, revision: RevisionInput) -> Arc<HirAnalysisOutput> {
    observe_cancel(db);
    let files = revision.files(db);
    let symbols = lower::symbols_in_files(db, files);
    lower::analysis_for_symbols(db, revision, &symbols)
}

#[salsa::tracked(returns(ref), no_eq)]
fn workspace_tir(db: &dyn SylDb, revision: RevisionInput) -> Arc<StageOutput<TirAnalysis>> {
    observe_cancel(db);
    let files = revision.files(db);
    let symbols = lower::symbols_in_files(db, files);
    lower::tir_for_symbols(db, revision, &symbols)
}

#[salsa::tracked(returns(ref), no_eq)]
fn unit_hir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Arc<HirAnalysisOutput> {
    observe_cancel(db);
    let files = lower::files_for_uris(db, revision, unit.closure_uris(db));
    let symbols = lower::symbols_in_files(db, &files);
    lower::analysis_for_symbols(db, revision, &symbols)
}

#[salsa::tracked(returns(ref), no_eq)]
fn unit_tir<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Arc<StageOutput<TirAnalysis>> {
    observe_cancel(db);
    let files = lower::files_for_uris(db, revision, unit.closure_uris(db));
    let symbols = lower::symbols_in_files(db, &files);
    lower::tir_for_symbols(db, revision, &symbols)
}

#[salsa::tracked(returns(ref), no_eq)]
fn unit_elaboration<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Option<Arc<ElaborationOutput>> {
    observe_cancel(db);
    let files = lower::files_for_uris(db, revision, unit.closure_uris(db));
    let hir = unit_hir(db, revision, unit);
    if !hir.diagnostics().is_empty() {
        return None;
    }
    let tir = unit_tir(db, revision, unit);
    if !tir.diagnostics().is_empty() {
        return None;
    }
    elab::design_output(db, revision, &files)
}

#[salsa::tracked(returns(ref), no_eq)]
fn workspace_elaboration(
    db: &dyn SylDb,
    revision: RevisionInput,
) -> Option<Arc<ElaborationOutput>> {
    observe_cancel(db);
    #[cfg(test)]
    {
        // The pause holds this query's claim. Cancellation is checked again
        // after the pause so a cancelled owner drops the claim without storing.
        pause::wait_if_armed();
        unwind_if_request_cancelled(db);
    }
    let hir = workspace_hir(db, revision);
    if !hir.diagnostics().is_empty() {
        return None;
    }
    let tir = workspace_tir(db, revision);
    if !tir.diagnostics().is_empty() {
        return None;
    }
    elab::design_output(db, revision, revision.files(db))
}

#[salsa::tracked(returns(ref))]
fn merged_opaque(db: &dyn SylDb, revision: RevisionInput) -> Option<OpaqueSummaryTable> {
    observe_cancel(db);
    let tir = workspace_tir(db, revision);
    if let Some(analysis) = tir.partial_stage() {
        return Some(analysis.opaque_summaries().merged(revision.opaque(db)));
    }
    let overlay = revision.opaque(db);
    if overlay.is_empty() {
        None
    } else {
        Some(overlay.clone())
    }
}

#[salsa::tracked(returns(ref), no_eq)]
fn workspace_diagnostics(db: &dyn SylDb, revision: RevisionInput) -> Vec<syl_span::Diagnostic> {
    observe_cancel(db);
    let hir = workspace_hir(db, revision);
    let tir = workspace_tir(db, revision);
    let elaboration = if hir.diagnostics().is_empty() && tir.diagnostics().is_empty() {
        workspace_elaboration(db, revision)
            .as_ref()
            .map(Arc::as_ref)
    } else {
        None
    };
    lower::diagnostics_from(hir, tir, elaboration)
}

#[salsa::tracked(returns(ref), no_eq)]
fn package_diagnostics<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Vec<syl_span::Diagnostic> {
    observe_cancel(db);
    let hir = unit_hir(db, revision, unit);
    let tir = unit_tir(db, revision, unit);
    let elaboration = if hir.diagnostics().is_empty() && tir.diagnostics().is_empty() {
        unit_elaboration(db, revision, unit)
            .as_ref()
            .map(Arc::as_ref)
    } else {
        None
    };
    lower::diagnostics_from(hir, tir, elaboration)
}

fn hir_for_owner(db: &dyn SylDb, revision: RevisionInput, symbol: SymbolId) -> HirAnalysisOutput {
    let module = symbol.module(db).clone();
    let Some(file) = lower::file_for_module(db, revision, &module) else {
        return SemanticSession::new(&[]).resolve_hir_partial();
    };
    let owner = owner_item(db, file, symbol);
    let files = lower::demanded_files(db, revision, file, symbol, &owner.0);
    lower::hir_from_sources(db, revision, files)
}
