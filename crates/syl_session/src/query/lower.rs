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
    let mut resolver = DemandResolver::new(db, revision);
    resolver.seed(owner_file, owner);
    resolver.resolve();
    resolver.assemble_files(owner, owner_item)
}

/// Full bodies elaboration must read, beyond this owner's own item.
///
/// Functions and maps are evaluated while lowering this owner. An `inplace`
/// cell is expanded into this module, so its body is an input. A `place`d
/// cell is a separate module and is not a companion.
pub(super) fn elab_companions<'db>(
    db: &'db dyn SylDb,
    revision: RevisionInput,
    owner_file: SourceFileInput,
    owner: SymbolId<'db>,
    owner_item: &Item,
) -> Vec<SymbolId<'db>> {
    let mut resolver = DemandResolver::new(db, revision);
    resolver.seed(owner_file, owner);
    resolver
        .inplace
        .extend(super::refs::inplace_callees(owner_item));
    resolver.resolve();
    resolver.expand_companion_bodies(owner);
    resolver.companions(owner)
}

/// Resolves the declaration and body closure needed by one owner query.
///
/// The resolver owns both the work queue and the companion scan state. Keeping
/// these together makes the traversal phases explicit: seed, resolve names,
/// then (when elaborating) expand the bodies of the selected companions.
struct DemandResolver<'db> {
    db: &'db dyn SylDb,
    revision: RevisionInput,
    owner_module: String,
    walk: DemandWalk,
    inplace: BTreeSet<String>,
    scanned: BTreeSet<(String, u8, String, u32)>,
}

impl<'db> DemandResolver<'db> {
    fn new(db: &'db dyn SylDb, revision: RevisionInput) -> Self {
        Self {
            db,
            revision,
            owner_module: String::new(),
            walk: DemandWalk {
                seen: BTreeSet::new(),
                queue: Vec::new(),
                modules: Vec::new(),
            },
            inplace: BTreeSet::new(),
            scanned: BTreeSet::new(),
        }
    }

    fn seed(&mut self, owner_file: SourceFileInput, owner: SymbolId) {
        self.owner_module = owner.module(self.db).clone();
        self.demand(&self.owner_module.clone(), owner.name(self.db));
        let text = super::owner_item_text(self.db, owner_file, owner);
        for name in identifiers(text) {
            self.demand_if_present(&self.owner_module.clone(), &name);
        }
    }

    fn resolve(&mut self) {
        loop {
            while let Some((module, name)) = self.walk.queue.pop() {
                self.expand_demand(&module, &name);
            }
            let before = self.walk.seen.len();
            self.demand_owner_uses();
            if self.walk.seen.len() == before {
                break;
            }
        }
    }

    /// A signature name can be brought into the owner by a `use` whose leaf is
    /// not written in the owner body. Only those uses are read, so an unused
    /// import stays out of the dependency set.
    fn demand_owner_uses(&mut self) {
        let names = self
            .walk
            .seen
            .iter()
            .map(|(_, name)| name.clone())
            .collect::<Vec<_>>();
        for name in names {
            if self
                .walk
                .seen
                .contains(&(self.owner_module.clone(), name.clone()))
            {
                continue;
            }
            let Some(file) = file_for_module(self.db, self.revision, &self.owner_module) else {
                continue;
            };
            let has_use = named_hits(self.db, file, &name)
                .iter()
                .any(|hit| matches!(hit.key.kind, SymbolKind::Use));
            if has_use {
                self.demand(&self.owner_module.clone(), &name);
            }
        }
    }

    fn demand(&mut self, module: &str, name: &str) {
        if !self
            .walk
            .seen
            .insert((module.to_string(), name.to_string()))
        {
            return;
        }
        if !self.walk.modules.iter().any(|seen| seen == module) {
            self.walk.modules.push(module.to_string());
        }
        self.walk.queue.push((module.to_string(), name.to_string()));
    }

    fn demand_if_present(&mut self, module: &str, name: &str) {
        if self
            .walk
            .seen
            .contains(&(module.to_string(), name.to_string()))
        {
            return;
        }
        let Some(file) = file_for_module(self.db, self.revision, module) else {
            return;
        };
        if named_hits(self.db, file, name).is_empty() {
            return;
        }
        self.demand(module, name);
    }

    fn expand_demand(&mut self, module: &str, name: &str) {
        let Some(file) = file_for_module(self.db, self.revision, module) else {
            return;
        };
        for hit in named_hits(self.db, file, name) {
            let symbol = intern_symbol(self.db, &hit.key);
            let contract = declaration_contract(self.db, self.revision, symbol);
            if let Some(path) = contract.use_path.clone() {
                let Some(target) = import_module(&path) else {
                    continue;
                };
                let Some(leaf) = path.last() else {
                    continue;
                };
                self.demand(&target, leaf);
                continue;
            }
            // Binders (parameter and generic names) are not reads. Type paths
            // and value expressions are. Tokenizing the signature text pulls
            // `const x` through `fn helper(x: nat)`.
            let signature = super::symbol_signature(self.db, self.revision, file, symbol);
            for ident in super::refs::signature_references(&signature.item) {
                self.demand_if_present(module, &ident);
            }
        }
    }

    /// Pull full bodies of functions, maps, and `inplace` cells the owner expands.
    fn expand_companion_bodies(&mut self, owner: SymbolId) {
        loop {
            let mut scanned_new = false;
            let names = self.walk.seen.iter().cloned().collect::<Vec<_>>();
            for (module, name) in names {
                let Some(file) = file_for_module(self.db, self.revision, &module) else {
                    continue;
                };
                for hit in named_hits(self.db, file, &name) {
                    let reads_body = match hit.key.kind {
                        SymbolKind::Fn | SymbolKind::Map => true,
                        SymbolKind::Cell | SymbolKind::ExternCell => {
                            self.inplace.contains(&hit.key.name)
                        }
                        _ => false,
                    };
                    if !reads_body {
                        continue;
                    }
                    let mark = (
                        hit.key.module.join("."),
                        hit.key.kind.tag(),
                        hit.key.name.clone(),
                        hit.key.discriminator,
                    );
                    if !self.scanned.insert(mark) || same_symbol(self.db, owner, &hit.key) {
                        continue;
                    }
                    scanned_new = true;
                    let symbol = intern_symbol(self.db, &hit.key);
                    let text = super::owner_item_text(self.db, file, symbol);
                    for ident in identifiers(text) {
                        self.demand_if_present(&module, &ident);
                    }
                    let item = super::owner_item(self.db, file, symbol);
                    for callee in super::refs::inplace_callees(&item.0) {
                        self.inplace.insert(callee.clone());
                        self.demand_if_present(&module, &callee);
                    }
                }
            }
            while let Some((module, name)) = self.walk.queue.pop() {
                self.expand_demand(&module, &name);
            }
            if !scanned_new {
                break;
            }
        }
    }

    fn companions<'a>(&'a self, owner: SymbolId) -> Vec<SymbolId<'db>> {
        let mut companions = Vec::new();
        let mut seen_ids = BTreeSet::new();
        for (module, name) in &self.walk.seen {
            let Some(file) = file_for_module(self.db, self.revision, module) else {
                continue;
            };
            for hit in named_hits(self.db, file, name) {
                let include = match hit.key.kind {
                    SymbolKind::Fn | SymbolKind::Map => true,
                    SymbolKind::Cell | SymbolKind::ExternCell => {
                        self.inplace.contains(&hit.key.name)
                    }
                    _ => false,
                };
                if !include || same_symbol(self.db, owner, &hit.key) {
                    continue;
                }
                let id = (
                    hit.key.module.join("."),
                    hit.key.kind.tag(),
                    hit.key.name.clone(),
                    hit.key.discriminator,
                );
                if seen_ids.insert(id) {
                    companions.push(intern_symbol(self.db, &hit.key));
                }
            }
        }
        companions
    }

    fn assemble_files(&self, owner: SymbolId, owner_item: &Item) -> Vec<(Vec<String>, AstFile)> {
        let owner_module = owner.module(self.db);
        let mut files = Vec::new();
        for module in &self.walk.modules {
            let Some(file) = file_for_module(self.db, self.revision, module) else {
                continue;
            };
            let mut hits = Vec::new();
            for (seen_module, name) in &self.walk.seen {
                if seen_module != module {
                    continue;
                }
                hits.extend(named_hits(self.db, file, name));
            }
            hits.sort_by_key(|hit| hit.index);
            hits.dedup_by_key(|hit| hit.index);
            if hits.is_empty() {
                continue;
            }
            let mut items = Vec::new();
            for hit in hits {
                let symbol = intern_symbol(self.db, &hit.key);
                if module == owner_module && same_symbol(self.db, owner, &hit.key) {
                    items.push(owner_item.clone());
                } else {
                    items.push(
                        super::symbol_signature(self.db, self.revision, file, symbol)
                            .item
                            .clone(),
                    );
                }
            }
            let source_id = SourceId::new(*file.source_id(self.db) as usize);
            let doc = super::file_module_doc(self.db, file).clone();
            files.push((
                file.module_path(self.db).clone(),
                AstFile::with_source_doc(source_id, doc, items),
            ));
        }
        files
    }
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
