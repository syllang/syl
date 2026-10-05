//! HIR assembly and contract reads for the tracked queries in the parent module.

use std::collections::BTreeSet;
use std::sync::Arc;

use syl_sema::{
    ContractProvider, DeclarationContract, HardwareContract, HirAnalysisOutput, SemanticSession,
    SemanticSourceFile, StageOutput, SymbolKey, SymbolKind, TirAnalysis,
};
use syl_syntax::{AstFile, Item};

use super::{
    RevisionInput, SignatureItem, SourceFileInput, SymbolId, UnitId, declaration_contract,
    file_contracts, hardware_contract, intern_symbol,
};
use crate::query::SylDb;
use syl_span::SourceId;

pub(super) fn diagnostics_from(
    hir: &HirAnalysisOutput,
    tir: &syl_sema::StageOutput<syl_sema::TirAnalysis>,
    elaboration: Option<&syl_elab::ElaborationOutput>,
) -> Vec<syl_span::Diagnostic> {
    if !hir.diagnostics().is_empty() {
        return hir.diagnostics().to_vec();
    }
    if !tir.diagnostics().is_empty() {
        return tir.diagnostics().to_vec();
    }
    elaboration
        .map(|output| output.diagnostics().to_vec())
        .unwrap_or_default()
}

/// One owner returns that owner's HIR. Several owners are remapped by `SymbolKey`
/// into the aggregate arena; the package façade does not lower the files again.
pub(super) fn analysis_for_symbols<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbols: &[SymbolId<'db>],
) -> Arc<HirAnalysisOutput> {
    if let [symbol] = symbols {
        return Arc::clone(super::owner_hir(db, revision, *symbol));
    }
    let owners = symbols
        .iter()
        .copied()
        .map(|symbol| {
            (
                key_from_symbol(db, symbol),
                Arc::clone(super::owner_hir(db, revision, symbol)),
            )
        })
        .collect::<Vec<_>>();
    let refs = owners
        .iter()
        .map(|(key, hir)| (key.clone(), hir.as_ref()))
        .collect::<Vec<_>>();
    Arc::new(syl_sema::aggregate_owner_hir(&refs))
}

pub(super) fn tir_for_symbols<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    symbols: &[SymbolId<'db>],
) -> Arc<StageOutput<TirAnalysis>> {
    if let [symbol] = symbols {
        return Arc::clone(super::owner_tir(db, revision, *symbol));
    }
    let owners = symbols
        .iter()
        .copied()
        .map(|symbol| {
            (
                key_from_symbol(db, symbol),
                Arc::clone(super::owner_tir(db, revision, symbol)),
            )
        })
        .collect::<Vec<_>>();
    let refs = owners
        .iter()
        .map(|(key, tir)| (key.clone(), tir.as_ref()))
        .collect::<Vec<_>>();
    Arc::new(syl_sema::aggregate_owner_tir(&refs))
}

pub(super) fn hir_from_sources(
    db: &dyn SylDb,
    revision: RevisionInput,
    files: Vec<(Vec<String>, AstFile)>,
) -> HirAnalysisOutput {
    let contracts = QueryContracts { db, revision };
    let sources = files
        .iter()
        .map(|(module, ast)| SemanticSourceFile::new(module.clone(), ast))
        .collect();
    SemanticSession::new_sources(sources).resolve_hir_partial_with_contracts(&contracts)
}

struct DemandWalk {
    seen: BTreeSet<(String, String)>,
    queue: Vec<(String, String)>,
    modules: Vec<String>,
}

/// Items the owner names, plus the signature closure of those declarations.
///
/// The owner module is one file. A cycle that names the owner again does not
/// append a second stub of that module.
pub(super) fn demanded_files(
    db: &dyn SylDb,
    revision: RevisionInput,
    owner_file: SourceFileInput,
    owner: SymbolId,
    owner_item: &Item,
) -> Vec<(Vec<String>, AstFile)> {
    let owner_module = owner.module(db).clone();
    let mut walk = DemandWalk {
        seen: BTreeSet::new(),
        queue: Vec::new(),
        modules: Vec::new(),
    };
    demand(&mut walk, &owner_module, owner.name(db));
    let text = super::owner_item_text(db, owner_file, owner);
    for name in identifiers(text) {
        demand_if_present(db, revision, &mut walk, &owner_module, &name);
    }
    loop {
        while let Some((module, name)) = walk.queue.pop() {
            expand_demand(db, revision, &mut walk, &module, &name);
        }
        let before = walk.seen.len();
        demand_owner_uses(db, revision, &mut walk, &owner_module);
        if walk.seen.len() == before {
            break;
        }
    }
    assemble_files(db, revision, owner, owner_item, &walk)
}

/// A signature name can be brought into the owner by a `use` whose leaf is not
/// written in the owner body. Only those uses are read, so an unused import
/// stays out of the dependency set.
fn demand_owner_uses(
    db: &dyn SylDb,
    revision: RevisionInput,
    walk: &mut DemandWalk,
    owner_module: &str,
) {
    let names = walk
        .seen
        .iter()
        .map(|(_, name)| name.clone())
        .collect::<Vec<_>>();
    for name in names {
        demand_owner_use(db, revision, walk, owner_module, &name);
    }
}

fn demand_owner_use(
    db: &dyn SylDb,
    revision: RevisionInput,
    walk: &mut DemandWalk,
    module: &str,
    name: &str,
) {
    if walk.seen.contains(&(module.to_string(), name.to_string())) {
        return;
    }
    let Some(file) = file_for_module(db, revision, module) else {
        return;
    };
    let has_use = named_hits(db, file, name)
        .iter()
        .any(|hit| matches!(hit.key.kind, SymbolKind::Use));
    if has_use {
        demand(walk, module, name);
    }
}

fn demand(walk: &mut DemandWalk, module: &str, name: &str) {
    if !walk.seen.insert((module.to_string(), name.to_string())) {
        return;
    }
    if !walk.modules.iter().any(|seen| seen == module) {
        walk.modules.push(module.to_string());
    }
    walk.queue.push((module.to_string(), name.to_string()));
}

fn demand_if_present(
    db: &dyn SylDb,
    revision: RevisionInput,
    walk: &mut DemandWalk,
    module: &str,
    name: &str,
) {
    if walk.seen.contains(&(module.to_string(), name.to_string())) {
        return;
    }
    let Some(file) = file_for_module(db, revision, module) else {
        return;
    };
    if named_hits(db, file, name).is_empty() {
        return;
    }
    demand(walk, module, name);
}

fn expand_demand(
    db: &dyn SylDb,
    revision: RevisionInput,
    walk: &mut DemandWalk,
    module: &str,
    name: &str,
) {
    let Some(file) = file_for_module(db, revision, module) else {
        return;
    };
    for hit in named_hits(db, file, name) {
        let symbol = intern_symbol(db, &hit.key);
        let contract = declaration_contract(db, revision, symbol);
        if let Some(path) = contract.use_path.clone() {
            let Some(target) = import_module(&path) else {
                continue;
            };
            let Some(leaf) = path.last() else {
                continue;
            };
            demand(walk, &target, leaf);
            continue;
        }
        // Binders (parameter and generic names) are not reads. Type paths and
        // value expressions are. Tokenizing the signature text pulls `const x`
        // through `fn helper(x: nat)`.
        let signature = super::symbol_signature(db, revision, file, symbol);
        for ident in super::refs::signature_references(&signature.item) {
            demand_if_present(db, revision, walk, module, &ident);
        }
    }
}

fn assemble_files(
    db: &dyn SylDb,
    revision: RevisionInput,
    owner: SymbolId,
    owner_item: &Item,
    walk: &DemandWalk,
) -> Vec<(Vec<String>, AstFile)> {
    let owner_module = owner.module(db);
    let mut files = Vec::new();
    for module in &walk.modules {
        let Some(file) = file_for_module(db, revision, module) else {
            continue;
        };
        let mut hits = Vec::new();
        for (seen_module, name) in &walk.seen {
            if seen_module != module {
                continue;
            }
            hits.extend(named_hits(db, file, name));
        }
        hits.sort_by_key(|hit| hit.index);
        hits.dedup_by_key(|hit| hit.index);
        if hits.is_empty() {
            continue;
        }
        let mut items = Vec::new();
        for hit in hits {
            let symbol = intern_symbol(db, &hit.key);
            if module == owner_module && same_symbol(db, owner, &hit.key) {
                items.push(owner_item.clone());
            } else {
                items.push(
                    super::symbol_signature(db, revision, file, symbol)
                        .item
                        .clone(),
                );
            }
        }
        let source_id = SourceId::new(*file.source_id(db) as usize);
        let doc = super::file_module_doc(db, file).clone();
        files.push((
            file.module_path(db).clone(),
            AstFile::with_source_doc(source_id, doc, items),
        ));
    }
    files
}

fn named_hits(db: &dyn SylDb, file: SourceFileInput, name: &str) -> Vec<super::NamedHit> {
    super::symbol_named(db, file, super::LookupName::new(db, name.to_string()))
        .hits
        .clone()
}

fn identifiers(text: &str) -> Vec<String> {
    text.split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
        .filter(|token| !token.is_empty() && !token.as_bytes()[0].is_ascii_digit())
        .map(str::to_string)
        .collect()
}

pub(super) fn signature_item(
    db: &dyn SylDb,
    revision: RevisionInput,
    file: SourceFileInput,
    symbol: SymbolId,
) -> SignatureItem {
    let item = super::owner_item(db, file, symbol);
    let source = item.0.span().source;
    let ast = AstFile::with_source_doc(source, None, vec![item.0.clone()]);
    let stub = syl_sema::signature_stub_file(&ast);
    let stub_item = stub
        .items
        .into_iter()
        .next()
        .unwrap_or_else(|| item.0.clone());
    let fingerprint = declaration_contract(db, revision, symbol)
        .signature_source
        .clone();
    SignatureItem::new(stub_item, fingerprint)
}

pub(super) fn local_symbols<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    unit: UnitId<'db>,
) -> Vec<SymbolId<'db>> {
    let local: BTreeSet<&str> = unit.local_uris(db).iter().map(String::as_str).collect();
    let files = revision
        .files(db)
        .iter()
        .copied()
        .filter(|file| local.contains(file.uri(db).as_str()))
        .collect::<Vec<_>>();
    symbols_in_files(db, &files)
}

pub(super) fn symbols_in_files<'db>(
    db: &'db dyn SylDb,
    files: &[SourceFileInput],
) -> Vec<SymbolId<'db>> {
    let mut symbols = Vec::new();
    for file in files {
        let index = super::file_item_index(db, *file);
        for key in &index.keys {
            if matches!(key.kind, SymbolKind::Use | SymbolKind::Error) {
                continue;
            }
            symbols.push(intern_symbol(db, key));
        }
    }
    symbols
}

pub(super) fn record_hardware(db: &dyn SylDb, revision: RevisionInput, files: &[SourceFileInput]) {
    let contracts = QueryContracts { db, revision };
    for file in files {
        let index = super::file_item_index(db, *file);
        for key in &index.keys {
            if matches!(
                key.kind,
                SymbolKind::Cell | SymbolKind::Fn | SymbolKind::Map
            ) {
                let _ = contracts.hardware(key);
            }
        }
    }
}

struct QueryContracts<'a> {
    db: &'a dyn SylDb,
    revision: RevisionInput,
}

impl ContractProvider for QueryContracts<'_> {
    fn declaration(&self, key: &SymbolKey) -> Option<DeclarationContract> {
        let symbol = intern_symbol(self.db, key);
        Some(declaration_contract(self.db, self.revision, symbol).clone())
    }

    fn hardware(&self, key: &SymbolKey) -> Option<HardwareContract> {
        let module = key.module.join(".");
        let file = file_for_module(self.db, self.revision, &module)?;
        let symbol = intern_symbol(self.db, key);
        Some(hardware_contract(self.db, file, symbol).clone())
    }

    fn declarations(&self, module: &[String]) -> Vec<DeclarationContract> {
        let Some(file) = file_for_module(self.db, self.revision, &module.join(".")) else {
            return Vec::new();
        };
        file_contracts(self.db, file)
            .contracts
            .iter()
            .filter_map(|contract| self.declaration(&contract.key))
            .collect()
    }

    fn imported_declaration(&self, module: &[String], name: &str) -> Option<DeclarationContract> {
        let file = file_for_module(self.db, self.revision, &module.join("."))?;
        // Per-name lookup backdates when that name is unchanged, so an edit to a
        // different item in the module does not dirty this owner.
        let hits = named_hits(self.db, file, name);
        let mut chosen = None;
        for hit in hits {
            let contract = self.declaration(&hit.key)?;
            if contract.signature_source.is_empty() && contract.use_path.is_none() {
                continue;
            }
            if chosen.is_none() {
                chosen = Some(contract);
            }
        }
        chosen
    }
}

fn import_module(path: &[String]) -> Option<String> {
    let module = if path.len() >= 2 {
        &path[..path.len() - 1]
    } else {
        path
    };
    if module.is_empty() {
        None
    } else {
        Some(module.join("."))
    }
}

pub(super) fn files_for_uris(
    db: &dyn SylDb,
    revision: RevisionInput,
    uris: &[String],
) -> Vec<SourceFileInput> {
    let wanted: BTreeSet<&str> = uris.iter().map(String::as_str).collect();
    revision
        .files(db)
        .iter()
        .copied()
        .filter(|file| wanted.contains(file.uri(db).as_str()))
        .collect()
}

pub(super) fn file_for_module(
    db: &dyn SylDb,
    revision: RevisionInput,
    module: &str,
) -> Option<SourceFileInput> {
    revision
        .files(db)
        .iter()
        .copied()
        .find(|file| file.module_path(db).join(".") == module)
}

pub(super) fn same_symbol(db: &dyn SylDb, symbol: SymbolId, key: &SymbolKey) -> bool {
    symbol.module(db) == &key.module.join(".")
        && *symbol.kind_tag(db) == key.kind.tag()
        && symbol.name(db) == &key.name
        && *symbol.discriminator(db) == key.discriminator
}

pub(super) fn key_from_symbol(db: &dyn SylDb, symbol: SymbolId) -> SymbolKey {
    SymbolKey::new(
        symbol
            .module(db)
            .split('.')
            .filter(|segment| !segment.is_empty())
            .map(str::to_string)
            .collect(),
        SymbolKind::from_tag(*symbol.kind_tag(db)),
        symbol.name(db).clone(),
        *symbol.discriminator(db),
    )
}

pub(super) fn empty_contract(symbol: SymbolId, db: &dyn SylDb) -> DeclarationContract {
    DeclarationContract::new(key_from_symbol(db, symbol), None, String::new(), None)
}
