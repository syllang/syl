use super::{context::CapabilityContext, model::EndpointSide, place::Place};
use crate::{
    CompileError, TirError,
    hir::{HirBodyExpr, HirCallable},
    ir::mir::{MirConstExprFacts, MirTypeRef},
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hir::{DefId, LocalId};

#[derive(Clone)]
pub(super) struct ViewFieldSchema {
    ty_label: String,
    fields: BTreeSet<String>,
}

#[derive(Clone, Default)]
pub(super) struct LocalTypeFacts {
    view_fields: BTreeMap<LocalId, ViewFieldSchema>,
}

impl LocalTypeFacts {
    pub(super) fn insert_view_fields(&mut self, local: LocalId, schema: ViewFieldSchema) {
        self.view_fields.insert(local, schema);
    }

    pub(super) fn require_known_field(&self, place: &Place) -> Result<(), CompileError> {
        require_known_field(self.view_fields.get(&place.root_id()), place)
    }
}

pub(super) struct ViewFieldSchemaRecord<'a> {
    pub(super) owner: DefId,
    pub(super) facts: &'a mut LocalTypeFacts,
    pub(super) local: LocalId,
    pub(super) ty: &'a MirTypeRef,
    pub(super) side: EndpointSide,
}

pub(super) fn record_view_field_schema(
    ctx: &dyn CapabilityContext,
    record: ViewFieldSchemaRecord<'_>,
) -> Result<(), CompileError> {
    let Some(schema) = resolve_view_field_schema(ctx, record.owner, record.ty, record.side)? else {
        return Ok(());
    };
    record.facts.insert_view_fields(record.local, schema);
    Ok(())
}

pub(super) fn record_let_view_field_schema(
    ctx: &dyn CapabilityContext,
    owner: DefId,
    facts: &mut LocalTypeFacts,
    local: LocalId,
    value: &HirBodyExpr,
) -> Result<(), CompileError> {
    let Some((callee_def, _, callable)) = callable_from_value(ctx, owner, value) else {
        return Ok(());
    };
    let Some(result_ty) = callable.result().map(|result| &result.ty) else {
        return Ok(());
    };
    let Some(schema) =
        resolve_view_field_schema(ctx, callee_def, result_ty, EndpointSide::Returned)?
    else {
        return Ok(());
    };
    facts.insert_view_fields(local, schema);
    Ok(())
}

pub(super) fn resolve_view_field_schema(
    ctx: &dyn CapabilityContext,
    owner: DefId,
    ty: &MirTypeRef,
    side: EndpointSide,
) -> Result<Option<ViewFieldSchema>, CompileError> {
    let Some(_) = ctx.view_caps(owner, ty, side)? else {
        return Ok(None);
    };
    let Some(fields) = ctx.view_field_names(owner, ty) else {
        return Ok(None);
    };
    Ok(Some(ViewFieldSchema {
        ty_label: type_label(ty),
        fields,
    }))
}

fn require_known_field(
    schema: Option<&ViewFieldSchema>,
    place: &Place,
) -> Result<(), CompileError> {
    let Some(field) = place.field() else {
        return Ok(());
    };
    let Some(schema) = schema else {
        return Ok(());
    };
    if schema.fields.contains(field) {
        return Ok(());
    }
    Err(CompileError::lowering_at(
        TirError::MissingAggregateField {
            ty: schema.ty_label.clone(),
            field: field.to_string(),
        },
        place.span(),
    ))
}

fn type_label(ty: &MirTypeRef) -> String {
    if let Some((base, view)) = ty.view_select() {
        return format!("{}.{}", type_label(base), view);
    }
    if let Some((len, elem)) = ty.array() {
        return format!("[{}] {}", len.fact_key(), type_label(elem));
    }
    if let Some(path) = ty.path() {
        let mut label = path.join(".");
        append_type_args(&mut label, ty.args());
        return label;
    }
    if let Some(base) = ty.generic_base() {
        let mut label = type_label(base);
        append_type_args(&mut label, ty.args());
        return label;
    }
    "<unknown>".to_string()
}

fn append_type_args(label: &mut String, args: Option<&[MirTypeRef]>) {
    let Some(args) = args else {
        return;
    };
    if args.is_empty() {
        return;
    }
    label.push('<');
    label.push_str(&args.iter().map(type_label).collect::<Vec<_>>().join(", "));
    label.push('>');
}

fn callable_from_value<'a>(
    ctx: &'a dyn CapabilityContext,
    owner: DefId,
    expr: &HirBodyExpr,
) -> Option<(DefId, String, &'a HirCallable)> {
    match &expr.node {
        crate::hir::HirExprNode::Call { callee, .. }
        | crate::hir::HirExprNode::Place { callee, .. } => callable_for_callee(ctx, owner, callee),
        _ => None,
    }
}

fn callable_for_callee<'a>(
    ctx: &'a dyn CapabilityContext,
    owner: DefId,
    callee: &HirBodyExpr,
) -> Option<(DefId, String, &'a HirCallable)> {
    let root = callee_root(callee)?;
    let Some(crate::hir::resolve::HirResolution::Def(def)) =
        ctx.expr_resolution(owner, root).ok()?
    else {
        return None;
    };
    let kind = ctx.def_kind(def)?;
    if !matches!(
        kind,
        crate::hir::HirDefKind::Cell | crate::hir::HirDefKind::ExternCell
    ) {
        return None;
    }
    let name = ctx.def_name(def)?.to_string();
    let callable = ctx.callable_by_def(def)?;
    Some((def, name, callable))
}

fn callee_root(mut callee: &HirBodyExpr) -> Option<&HirBodyExpr> {
    loop {
        match &callee.node {
            crate::hir::HirExprNode::Ident(_) => return Some(callee),
            crate::hir::HirExprNode::GenericApp { callee: inner, .. }
            | crate::hir::HirExprNode::Group(inner) => callee = inner,
            _ => return None,
        }
    }
}
