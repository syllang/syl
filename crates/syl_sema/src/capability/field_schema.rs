use super::{context::CapabilityContext, model::EndpointSide, place::Place};
use crate::{
    CompileError, TirError,
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
