//! HIR arena payload copying and identifier remapping.
//!
//! This module owns the translation of owner-local ids into the aggregated HIR
//! arena. The parent aggregation module decides which definitions win; this
//! module only applies the resulting [`Remap`] to payloads.

use syl_hir::{DefId, ExprId, HirAttribute, HirResolution, LocalId};

use super::Remap;
use crate::hir::HirDesign;
use crate::hir::{
    HirBlock, HirBodyExpr, HirBundleItem, HirCallable, HirConstItem, HirEnumItem,
    HirEnumVariantKey, HirExpr, HirExprNode, HirFieldAccess, HirFnItem, HirInterfaceItem, HirLocal,
    HirMapItem, HirMatchArm, HirMemberDecl, HirSelectArm, HirSignatureGenericParam,
    HirSignatureParam, HirSignatureResultBinding, HirStmt, HirStructItem, HirTypeRef,
};

pub(super) fn copy_payload(
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
