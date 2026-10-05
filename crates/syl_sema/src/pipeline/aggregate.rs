//! Merge per-owner HIR/TIR into one arena.
//!
//! Each owner result has its own arena. `SymbolKey` picks the full definition;
//! stub copies of that same key are dropped and cross-references are rewritten
//! to the aggregated id.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use syl_hir::{DefId, ExprId, HirAttribute, HirResolution, LocalId, PackageId};
use syl_span::Diagnostic;

use crate::contract::{SymbolKey, SymbolKind};
use crate::hir::{
    HirBlock, HirBodyExpr, HirBundleItem, HirCallable, HirConstItem, HirDef, HirDefKind, HirDesign,
    HirEnumItem, HirEnumVariantKey, HirExpr, HirExprNode, HirFieldAccess, HirFnItem,
    HirInterfaceItem, HirLocal, HirMapItem, HirMatchArm, HirMemberDecl, HirSelectArm,
    HirSignatureGenericParam, HirSignatureParam, HirSignatureResultBinding, HirStmt, HirStructItem,
    HirTypeRef,
};
use crate::pipeline::analysis::{HirAnalysis, TirAnalysis};
use crate::pipeline::output::{HirAnalysisOutput, StageOutput};
use crate::tir::{BindingRef, RemappedTirTables, TirDesign, TirTypeTable, TypeId};

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
    let (design, diagnostics, _) = aggregate_slices(&slices);
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
    let (design, diagnostics, remaps) = aggregate_slices(&slices);
    let tir = remap_tir(Arc::new(design), &slices, &remaps);
    StageOutput::new(Some(TirAnalysis::new(tir)), diagnostics)
}

fn aggregate_slices(slices: &[OwnerSlice<'_>]) -> (HirDesign, Vec<Diagnostic>, Vec<Remap>) {
    let chosen = choose(slices);
    let mut remaps = slices
        .iter()
        .enumerate()
        .map(|(index, slice)| remap_for(slice.design, &chosen, index))
        .collect::<Vec<_>>();
    let mut design = HirDesign::empty();
    for (new_index, item) in chosen.iter().enumerate() {
        let source = slices[item.owner].design;
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
        copy_payload(
            &mut design,
            source,
            item.old_def,
            new_id,
            &mut remaps[item.owner],
        );
    }
    copy_imports(&mut design, slices);
    register_extensions(&mut design);
    (design, deduped_diagnostics(slices), remaps)
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

fn copy_payload(
    dest: &mut HirDesign,
    source: &HirDesign,
    old_def: DefId,
    new_id: DefId,
    remap: &mut Remap,
) {
    for local in source.locals() {
        if local.owner != old_def {
            continue;
        }
        let id = LocalId::new(dest.locals().len());
        remap.locals.insert(local.id, id);
        dest.locals_mut().push(HirLocal::new(
            id,
            new_id,
            local.name.clone(),
            local.kind,
            local.span,
        ));
    }
    for expr in source.exprs() {
        if expr.owner != old_def {
            continue;
        }
        let id = ExprId::new(dest.exprs().len());
        remap.exprs.insert(expr.id, id);
        dest.exprs_mut().push(HirExpr::new(id, new_id, expr.span));
    }
    if let Some(item) = source.consts().get(&old_def).cloned() {
        dest.consts_mut().insert(new_id, remap_const(item, remap));
    }
    if let Some(item) = source.fns().get(&old_def).cloned() {
        dest.fns_mut().insert(new_id, remap_fn(item, remap));
    }
    if let Some(item) = source.enums().get(&old_def).cloned() {
        dest.enums_mut().insert(new_id, remap_enum(item, remap));
    }
    if let Some(item) = source.structs().get(&old_def).cloned() {
        dest.structs_mut().insert(new_id, remap_struct(item, remap));
    }
    if let Some(item) = source.bundles().get(&old_def).cloned() {
        dest.bundles_mut().insert(new_id, remap_bundle(item, remap));
    }
    if let Some(item) = source.interfaces().get(&old_def).cloned() {
        dest.interfaces_mut()
            .insert(new_id, remap_interface(item, remap));
    }
    if let Some(item) = source.maps().get(&old_def).cloned() {
        dest.maps_mut().insert(new_id, remap_map(item, remap));
    }
    if let Some(item) = source.callables().get(&old_def).cloned() {
        dest.callables_mut()
            .insert(new_id, remap_callable(item, remap));
    }
    for (key, variant) in source.enum_variants() {
        if key.enum_def != old_def {
            continue;
        }
        let mut copied = variant.clone();
        copied.enum_def = new_id;
        dest.enum_variants_mut()
            .insert(HirEnumVariantKey::new(new_id, key.name.clone()), copied);
    }
    for member in source.member_decls() {
        if member.owner != old_def {
            continue;
        }
        dest.member_decls_mut().push(HirMemberDecl::with_doc(
            new_id,
            member.doc.clone(),
            member.name.clone(),
            member.kind.clone(),
            member.span,
        ));
    }
    for type_ref in source.type_refs() {
        if type_ref.owner != old_def {
            continue;
        }
        dest.type_refs_mut()
            .push(HirTypeRef::new(new_id, type_ref.ty.clone()));
    }
    for access in source.field_accesses() {
        if access.owner != old_def {
            continue;
        }
        let mut base = access.base.clone();
        remap_expr(&mut base, remap);
        dest.field_accesses_mut().push(HirFieldAccess::new(
            new_id,
            base,
            access.field.clone(),
            access.span,
        ));
    }
    for (expr_id, resolution) in source.expr_resolutions() {
        let Some(new_expr) = remap.exprs.get(expr_id).copied() else {
            continue;
        };
        let Some(mapped) = remap_resolution(*resolution, remap) else {
            continue;
        };
        dest.expr_resolutions_mut().insert(new_expr, mapped);
    }
}

fn remap_resolution(resolution: HirResolution, remap: &Remap) -> Option<HirResolution> {
    match resolution {
        HirResolution::Def(id) => remap.defs.get(&id).copied().map(HirResolution::Def),
        HirResolution::Local(id) => remap.locals.get(&id).copied().map(HirResolution::Local),
        _ => None,
    }
}

fn map_local(id: Option<LocalId>, remap: &Remap) -> Option<LocalId> {
    id.and_then(|id| remap.locals.get(&id).copied())
}

fn remap_const(mut item: HirConstItem, remap: &Remap) -> HirConstItem {
    remap_expr(&mut item.value, remap);
    item
}

fn remap_fn(mut item: HirFnItem, remap: &Remap) -> HirFnItem {
    remap_params(&mut item.params, remap);
    remap_block(&mut item.body, remap);
    item
}

fn remap_enum(mut item: HirEnumItem, remap: &Remap) -> HirEnumItem {
    for variant in &mut item.variants {
        if let Some(value) = variant.value.as_mut() {
            remap_expr(value, remap);
        }
    }
    item
}

fn remap_struct(mut item: HirStructItem, remap: &Remap) -> HirStructItem {
    remap_generics(&mut item.generics, remap);
    item
}

fn remap_bundle(mut item: HirBundleItem, remap: &Remap) -> HirBundleItem {
    remap_generics(&mut item.generics, remap);
    for attr in &mut item.attrs {
        remap_attribute(attr, remap);
    }
    item
}

fn remap_interface(mut item: HirInterfaceItem, remap: &Remap) -> HirInterfaceItem {
    remap_generics(&mut item.generics, remap);
    item
}

fn remap_map(mut item: HirMapItem, remap: &Remap) -> HirMapItem {
    remap_generics(&mut item.generics, remap);
    remap_params(&mut item.params, remap);
    remap_expr(&mut item.body, remap);
    item
}

fn remap_callable(item: HirCallable, remap: &Remap) -> HirCallable {
    match item {
        HirCallable::Cell(mut item) => {
            remap_generics(&mut item.generics, remap);
            remap_params(&mut item.params, remap);
            if let Some(result) = item.result.as_mut() {
                remap_result(result, remap);
            }
            remap_block(&mut item.body, remap);
            HirCallable::Cell(item)
        }
        HirCallable::Extern(mut item) => {
            remap_generics(&mut item.generics, remap);
            remap_params(&mut item.params, remap);
            if let Some(result) = item.result.as_mut() {
                remap_result(result, remap);
            }
            HirCallable::Extern(item)
        }
        other => other,
    }
}

fn remap_generics(generics: &mut [HirSignatureGenericParam], remap: &Remap) {
    for generic in generics {
        generic.id = map_local(generic.id, remap);
        if let Some(default) = generic.default.as_mut() {
            remap_expr(default, remap);
        }
    }
}

fn remap_params(params: &mut [HirSignatureParam], remap: &Remap) {
    for param in params {
        param.id = map_local(param.id, remap);
    }
}

fn remap_result(result: &mut HirSignatureResultBinding, remap: &Remap) {
    result.id = map_local(result.id, remap);
}

fn remap_attribute(attr: &mut HirAttribute, remap: &Remap) {
    for arg in &mut attr.args {
        remap_expr(arg, remap);
    }
}

fn remap_block(block: &mut HirBlock, remap: &Remap) {
    for stmt in &mut block.stmts {
        remap_stmt(stmt, remap);
    }
    if let Some(tail) = block.tail.as_deref_mut() {
        remap_expr(tail, remap);
    }
}

fn remap_stmt(stmt: &mut HirStmt, remap: &Remap) {
    match stmt {
        HirStmt::Const { id, value, .. } => {
            *id = map_local(*id, remap);
            remap_expr(value, remap);
        }
        HirStmt::Let { id, value, .. } | HirStmt::Var { id, value, .. } => {
            *id = map_local(*id, remap);
            if let Some(value) = value {
                remap_expr(value, remap);
            }
        }
        HirStmt::Signal { id, value, .. } => {
            *id = map_local(*id, remap);
            if let Some(value) = value {
                remap_expr(value, remap);
            }
        }
        HirStmt::Reg { id, reset, .. } => {
            *id = map_local(*id, remap);
            if let Some(reset) = reset {
                if let Some(domain) = reset.domain.as_mut() {
                    remap_expr(domain, remap);
                }
                remap_expr(&mut reset.value, remap);
            }
        }
        HirStmt::Assign { target, value, .. } | HirStmt::Drive { target, value, .. } => {
            remap_expr(target, remap);
            remap_expr(value, remap);
        }
        HirStmt::Next { value, .. } => remap_expr(value, remap),
        HirStmt::While { cond, body, .. } => {
            remap_expr(cond, remap);
            remap_block(body, remap);
        }
        HirStmt::ElabIf {
            cond,
            then_block,
            else_block,
            ..
        } => {
            remap_expr(cond, remap);
            remap_block(then_block, remap);
            if let Some(block) = else_block {
                remap_block(block, remap);
            }
        }
        HirStmt::ElabFor {
            id, range, body, ..
        } => {
            *id = map_local(*id, remap);
            remap_expr(range, remap);
            remap_block(body, remap);
        }
        HirStmt::Expr(expr) => remap_expr(expr, remap),
        HirStmt::Return(Some(expr), _) => remap_expr(expr, remap),
        HirStmt::Return(None, _) => {}
        HirStmt::Error { .. } => {}
        _ => {}
    }
}

fn remap_expr(expr: &mut HirBodyExpr, remap: &Remap) {
    if let Some(id) = remap.exprs.get(&expr.id).copied() {
        expr.id = id;
    }
    match &mut expr.node {
        HirExprNode::Unary { expr, .. }
        | HirExprNode::Group(expr)
        | HirExprNode::CompileError { message: expr } => {
            remap_expr(expr, remap);
        }
        HirExprNode::Binary { left, right, .. }
        | HirExprNode::Index {
            base: left,
            index: right,
        }
        | HirExprNode::Range {
            start: left,
            end: right,
        } => {
            remap_expr(left, remap);
            remap_expr(right, remap);
        }
        HirExprNode::Call { callee, args } | HirExprNode::Place { callee, args, .. } => {
            remap_expr(callee, remap);
            for arg in args {
                remap_expr(&mut arg.value, remap);
            }
        }
        HirExprNode::GenericApp { callee, .. } | HirExprNode::Field { base: callee, .. } => {
            remap_expr(callee, remap);
        }
        HirExprNode::Aggregate { fields, .. } => {
            for field in fields {
                remap_expr(&mut field.value, remap);
            }
        }
        HirExprNode::Block(block) => remap_block(block, remap),
        HirExprNode::Match { expr, arms } => {
            remap_expr(expr, remap);
            for HirMatchArm { value, .. } in arms {
                remap_expr(value, remap);
            }
        }
        HirExprNode::Select { arms, .. } => {
            for HirSelectArm { pattern, value, .. } in arms {
                remap_expr(pattern, remap);
                remap_expr(value, remap);
            }
        }
        HirExprNode::For {
            id, range, body, ..
        } => {
            *id = map_local(*id, remap);
            remap_expr(range, remap);
            remap_block(body, remap);
        }
        HirExprNode::Ident(_)
        | HirExprNode::Int(_)
        | HirExprNode::Str(_)
        | HirExprNode::Bool(_)
        | HirExprNode::Unsupported => {}
        _ => {}
    }
}

fn remap_tir(hir: Arc<HirDesign>, slices: &[OwnerSlice<'_>], remaps: &[Remap]) -> TirDesign {
    let mut type_table = TirTypeTable::new();
    let mut enum_values = std::collections::BTreeMap::new();
    let mut phases = std::collections::BTreeMap::new();
    let mut expr_types = std::collections::BTreeMap::new();
    let mut binding_kinds = std::collections::BTreeMap::new();
    let mut binding_types = std::collections::BTreeMap::new();
    for (index, slice) in slices.iter().enumerate() {
        let Some(tir) = slice.tir else {
            continue;
        };
        let remap = &remaps[index];
        let mut cache = HashMap::<TypeId, TypeId>::new();
        for (key, value) in tir.enum_variant_values() {
            if !remap.copied.contains(&key.enum_def) {
                continue;
            }
            let Some(new_def) = remap.defs.get(&key.enum_def).copied() else {
                continue;
            };
            enum_values.insert(HirEnumVariantKey::new(new_def, key.name.clone()), *value);
        }
        for (expr, phase) in tir.expr_phases() {
            if let Some(new_expr) = remap.exprs.get(expr).copied() {
                phases.insert(new_expr, *phase);
            }
        }
        for (expr, ty) in tir.expr_types() {
            if let Some(new_expr) = remap.exprs.get(expr).copied() {
                expr_types.insert(
                    new_expr,
                    intern_type(*ty, tir, remap, &mut cache, &mut type_table),
                );
            }
        }
        for (binding, kind) in tir.binding_kinds() {
            if let Some(mapped) = remap_binding(*binding, remap) {
                binding_kinds.insert(mapped, *kind);
            }
        }
        for (binding, ty) in tir.binding_types() {
            if let Some(mapped) = remap_binding(*binding, remap) {
                binding_types.insert(
                    mapped,
                    intern_type(*ty, tir, remap, &mut cache, &mut type_table),
                );
            }
        }
    }
    TirDesign::from_remapped(
        hir,
        RemappedTirTables {
            type_table,
            enum_variant_values: enum_values,
            expr_phases: phases,
            expr_types,
            binding_kinds,
            binding_types,
        },
    )
}

fn remap_binding(binding: BindingRef, remap: &Remap) -> Option<BindingRef> {
    match binding {
        BindingRef::Def(id) if remap.copied.contains(&id) => {
            remap.defs.get(&id).copied().map(BindingRef::Def)
        }
        BindingRef::Local(id) => remap.locals.get(&id).copied().map(BindingRef::Local),
        BindingRef::Def(_) => None,
    }
}

fn intern_type(
    old: TypeId,
    tir: &TirDesign,
    remap: &Remap,
    cache: &mut HashMap<TypeId, TypeId>,
    dest: &mut TirTypeTable,
) -> TypeId {
    if let Some(id) = cache.get(&old).copied() {
        return id;
    }
    let ty = tir
        .type_table()
        .get(old)
        .cloned()
        .unwrap_or(crate::tir::TirType::Unknown);
    let remapped = ty.remap_ids(&|id| remap.defs.get(&id).copied(), &|id| {
        remap.locals.get(&id).copied()
    });
    let id = dest.intern(remapped);
    cache.insert(old, id);
    id
}
