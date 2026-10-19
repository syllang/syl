//! Merge per-owner HIR/TIR into one arena.
//!
//! Each owner result has its own arena. `SymbolKey` picks the full definition;
//! stub copies of that same key are dropped and cross-references are rewritten
//! to the aggregated id.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use syl_hir::{DefId, ExprId, LocalId, PackageId};
use syl_span::Diagnostic;

use crate::contract::{SymbolKey, SymbolKind};
use crate::hir::{HirDef, HirDefKind, HirDesign};
use crate::pipeline::analysis::{HirAnalysis, TirAnalysis};
use crate::pipeline::output::{HirAnalysisOutput, StageOutput};
use crate::tir::TirDesign;

struct OwnerSlice<'a> {
    key: &'a SymbolKey,
    design: &'a HirDesign,
    diagnostics: &'a [Diagnostic],
    tir: Option<&'a TirDesign>,
}

struct Chosen {
    owner: usize,
    old_def: DefId,
    key: SymbolKey,
}

struct Remap {
    defs: HashMap<DefId, DefId>,
    locals: HashMap<LocalId, LocalId>,
    exprs: HashMap<ExprId, ExprId>,
    copied: HashSet<DefId>,
}

/// One HIR arena whose definitions come from the owner results.
pub fn aggregate_owner_hir(owners: &[(SymbolKey, &HirAnalysisOutput)]) -> HirAnalysisOutput {
    let slices = owners
        .iter()
        .map(|(key, output)| OwnerSlice {
            key,
            design: output.stage().design(),
            diagnostics: output.diagnostics(),
            tir: None,
        })
        .collect::<Vec<_>>();
    let (design, diagnostics) = OwnerAggregation::new(slices).finish().into_hir();
    HirAnalysisOutput::new(HirAnalysis::new(design), diagnostics)
}

/// One TIR arena whose tables are the owner tables, retargeted onto the aggregated HIR.
pub fn aggregate_owner_tir(
    owners: &[(SymbolKey, &StageOutput<TirAnalysis>)],
) -> StageOutput<TirAnalysis> {
    let slices = owners
        .iter()
        .filter_map(|(key, output)| {
            let tir = output.stage()?;
            Some(OwnerSlice {
                key,
                design: tir.design().hir(),
                diagnostics: output.diagnostics(),
                tir: Some(tir.design()),
            })
        })
        .collect::<Vec<_>>();
    let (tir, diagnostics) = OwnerAggregation::new(slices).finish().into_tir();
    StageOutput::new(Some(TirAnalysis::new(tir)), diagnostics)
}

/// Stateful aggregation context. It owns the definition selection and all
/// owner-local remaps for one aggregation operation. Callers can only obtain a
/// completed HIR arena and (for TIR) the corresponding remaps from this
/// context, which keeps selection and translation in the same phase boundary.
struct OwnerAggregation<'a> {
    slices: Vec<OwnerSlice<'a>>,
    chosen: Vec<Chosen>,
    remaps: Vec<Remap>,
}

impl<'a> OwnerAggregation<'a> {
    fn new(slices: Vec<OwnerSlice<'a>>) -> Self {
        let chosen = choose(&slices);
        let remaps = slices
            .iter()
            .enumerate()
            .map(|(index, slice)| remap_for(slice.design, &chosen, index))
            .collect();
        Self {
            slices,
            chosen,
            remaps,
        }
    }

    fn finish(mut self) -> AggregatedHir<'a> {
        let mut design = HirDesign::empty();
        for (new_index, item) in self.chosen.iter().enumerate() {
            let source = self.slices[item.owner].design;
            let Some(def) = source.def(item.old_def) else {
                continue;
            };
            let new_id = DefId::new(new_index);
            ensure_package(&mut design, source, def);
            if let Some(doc) = source.doc_for_module(def.span.source) {
                design
                    .module_docs_mut()
                    .entry(def.span.source)
                    .or_insert_with(|| doc.to_string());
            }
            design
                .def_names_mut()
                .entry(def.name.clone())
                .or_default()
                .push(new_id);
            design
                .canonical_def_names_mut()
                .insert(def.canonical_path.clone(), new_id);
            design.defs_mut().push(HirDef::new(
                new_id,
                def.name.clone(),
                def.canonical_path.clone(),
                def.kind,
                def.span,
            ));
            aggregate_remap::copy_payload(
                &mut design,
                source,
                item.old_def,
                new_id,
                &mut self.remaps[item.owner],
            );
        }
        copy_imports(&mut design, &self.slices);
        register_extensions(&mut design);
        AggregatedHir {
            design,
            diagnostics: deduped_diagnostics(&self.slices),
            slices: self.slices,
            remaps: self.remaps,
        }
    }
}

struct AggregatedHir<'a> {
    design: HirDesign,
    diagnostics: Vec<Diagnostic>,
    slices: Vec<OwnerSlice<'a>>,
    remaps: Vec<Remap>,
}

impl<'a> AggregatedHir<'a> {
    fn into_hir(self) -> (HirDesign, Vec<Diagnostic>) {
        (self.design, self.diagnostics)
    }

    fn into_tir(self) -> (TirDesign, Vec<Diagnostic>) {
        let Self {
            design,
            diagnostics,
            slices,
            remaps,
        } = self;
        (
            aggregate_tir::remap_tir(Arc::new(design), &slices, &remaps),
            diagnostics,
        )
    }
}

fn choose(slices: &[OwnerSlice<'_>]) -> Vec<Chosen> {
    let owner_keys = slices
        .iter()
        .map(|slice| slice.key.clone())
        .collect::<HashSet<_>>();
    let mut chosen = Vec::new();
    let mut index_of = HashMap::<SymbolKey, usize>::new();
    for (owner, slice) in slices.iter().enumerate() {
        for def_index in 0..slice.design.defs().len() {
            let key = symbol_key_for(slice.design, def_index);
            let old_def = slice.design.defs()[def_index].id;
            if &key == slice.key {
                if let Some(slot) = index_of.get(&key).copied() {
                    chosen[slot] = Chosen {
                        owner,
                        old_def,
                        key,
                    };
                } else {
                    index_of.insert(key.clone(), chosen.len());
                    chosen.push(Chosen {
                        owner,
                        old_def,
                        key,
                    });
                }
            } else if !owner_keys.contains(&key) && !index_of.contains_key(&key) {
                index_of.insert(key.clone(), chosen.len());
                chosen.push(Chosen {
                    owner,
                    old_def,
                    key,
                });
            }
        }
    }
    chosen
}

fn remap_for(design: &HirDesign, chosen: &[Chosen], owner: usize) -> Remap {
    let keys = chosen
        .iter()
        .enumerate()
        .map(|(slot, item)| (item.key.clone(), DefId::new(slot)))
        .collect::<HashMap<_, _>>();
    let mut defs = HashMap::new();
    for def_index in 0..design.defs().len() {
        let key = symbol_key_for(design, def_index);
        if let Some(new_id) = keys.get(&key).copied() {
            defs.insert(design.defs()[def_index].id, new_id);
        }
    }
    let copied = chosen
        .iter()
        .filter(|item| item.owner == owner)
        .map(|item| item.old_def)
        .collect();
    Remap {
        defs,
        locals: HashMap::new(),
        exprs: HashMap::new(),
        copied,
    }
}

fn symbol_key_for(design: &HirDesign, def_index: usize) -> SymbolKey {
    let def = &design.defs()[def_index];
    let module = def.canonical_path.parent().segments().to_vec();
    let kind = symbol_kind(def.kind);
    let discriminator = design.defs()[..def_index]
        .iter()
        .filter(|earlier| {
            earlier.kind == def.kind
                && earlier.name == def.name
                && earlier.canonical_path.parent().segments() == module.as_slice()
        })
        .count() as u32;
    SymbolKey::new(module, kind, def.name.clone(), discriminator)
}

fn symbol_kind(kind: HirDefKind) -> SymbolKind {
    match kind {
        HirDefKind::Const => SymbolKind::Const,
        HirDefKind::Fn => SymbolKind::Fn,
        HirDefKind::Enum => SymbolKind::Enum,
        HirDefKind::Bundle => SymbolKind::Bundle,
        HirDefKind::Struct => SymbolKind::Struct,
        HirDefKind::Interface => SymbolKind::Interface,
        HirDefKind::Map => SymbolKind::Map,
        HirDefKind::Cell => SymbolKind::Cell,
        HirDefKind::ExternCell => SymbolKind::ExternCell,
        _ => SymbolKind::Error,
    }
}

fn ensure_package(dest: &mut HirDesign, source: &HirDesign, def: &HirDef) {
    let path = def.canonical_path.parent().segments().to_vec();
    if dest.packages().iter().any(|package| package.path == path) {
        return;
    }
    let span = source
        .packages()
        .iter()
        .find(|package| package.path == path)
        .map(|package| package.span)
        .unwrap_or(def.span);
    let id = PackageId::new(dest.packages().len());
    dest.packages_mut()
        .push(crate::hir::HirPackage::new(id, path, span));
}

fn copy_imports(dest: &mut HirDesign, slices: &[OwnerSlice<'_>]) {
    let mut seen = HashSet::new();
    for slice in slices {
        for import in slice.design.imports() {
            let key = (import.package_path.segments().to_vec(), import.path.clone());
            if !seen.insert(key) {
                continue;
            }
            dest.imports_mut().push(crate::hir::HirImport::new(
                import.path.clone(),
                import.package_path.clone(),
                import.span,
            ));
        }
    }
}

fn deduped_diagnostics(slices: &[OwnerSlice<'_>]) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for slice in slices {
        for diagnostic in slice.diagnostics {
            if !diagnostics.contains(diagnostic) {
                diagnostics.push(diagnostic.clone());
            }
        }
    }
    diagnostics
}

fn register_extensions(design: &mut HirDesign) {
    let mut found = Vec::new();
    for (owner, item) in design.fns() {
        if let Some(param) = item.params.first().filter(|param| param.is_receiver()) {
            found.push((*owner, item.name.clone(), param.ty.clone()));
        }
    }
    for (owner, item) in design.maps() {
        if let Some(param) = item.params.first().filter(|param| param.is_receiver()) {
            found.push((*owner, item.name.clone(), param.ty.clone()));
        }
    }
    for (owner, name, ty) in found {
        if let Some(receiver) = design.type_def_for_mir_type(owner, &ty) {
            design.register_extension_method(receiver, name, owner);
        }
    }
}

#[path = "aggregate_remap.rs"]
mod aggregate_remap;
#[path = "aggregate_tir.rs"]
mod aggregate_tir;
