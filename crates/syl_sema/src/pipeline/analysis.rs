use super::output::StageOutput;
use crate::{
    CompileError,
    facts::{ResolutionTable, SemanticFacts},
    hir::{HirDef, HirDesign, HirExpr},
    query_support::{CompletionItem, CompletionKind},
    summary::opaque::OpaqueSummaryTable,
    tir::{TirDesign, TypePhaseChecker},
};
use std::{fmt, sync::Arc};
use syl_hir::{DefId, HirResolution};
use syl_span::{SourceId, Span};

#[non_exhaustive]
pub struct HirAnalysis {
    design: Arc<HirDesign>,
    resolution: ResolutionTable,
}

impl HirAnalysis {
    pub(super) fn new(design: HirDesign) -> Self {
        let resolution = ResolutionTable::collect(&design);
        Self {
            design: Arc::new(design),
            resolution,
        }
    }

    pub fn resolution(&self) -> &ResolutionTable {
        &self.resolution
    }

    pub fn def_count(&self) -> usize {
        self.design.defs.len()
    }

    pub fn local_count(&self) -> usize {
        self.design.locals.len()
    }

    pub fn doc_for_item(&self, def_id: DefId) -> Option<&str> {
        self.design.doc_for_item(def_id)
    }

    pub fn doc_for_field(&self, def_id: DefId, field: &str) -> Option<&str> {
        self.design.doc_for_field(def_id, field)
    }

    pub fn doc_for_module(&self, source_id: SourceId) -> Option<&str> {
        self.design.doc_for_module(source_id)
    }

    pub fn debug_dump(&self) -> String {
        self.design.debug_dump()
    }

    pub fn check_tir(&self) -> Result<TirAnalysis, CompileError> {
        TypePhaseChecker::new(Arc::clone(&self.design))
            .check()
            .map(TirAnalysis::new)
    }

    pub fn check_tir_partial(&self) -> StageOutput<TirAnalysis> {
        TypePhaseChecker::new(Arc::clone(&self.design))
            .check_output()
            .map_stage(TirAnalysis::new)
    }

    pub fn definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        if let Some(definition) = self.import_definition_at(span) {
            return Some(definition);
        }
        if let Some(definition) = self.expression_definition_at(span) {
            return Some(definition);
        }
        if let Some(definition) = self.member_definition_at(span) {
            return Some(definition);
        }
        if let Some(definition) = self.member_decl_definition_at(span) {
            return Some(definition);
        }
        if let Some(definition) = self.def_decl_definition_at(span) {
            return Some(definition);
        }
        self.type_definition_at(span)
    }

    pub fn hover_at(&self, span: Span) -> Option<HoverInfo> {
        let definition = self.definition_at(span)?;
        Some(HoverInfo::new(definition.span, definition.hover_text()))
    }

    pub fn completion_items(&self) -> Vec<CompletionItem> {
        self.completion_items_for_defs(self.design.defs.iter().map(|def| def.id), None)
    }

    pub fn completion_items_at(&self, span: Span) -> Vec<CompletionItem> {
        let owner = self.owner_at(span).map(|def| def.id);
        let def_ids = owner
            .map(|owner| self.design.visible_def_ids(owner))
            .unwrap_or_else(|| self.design.source_def_ids(span.source));
        self.completion_items_for_defs(def_ids, owner.map(|owner| (owner, span)))
    }

    pub fn member_completion_items_at(&self, span: Span) -> Vec<CompletionItem> {
        let Some(owner) = self.owner_at(span).map(|def| def.id) else {
            return Vec::new();
        };
        self.design
            .member_completion_fields_at(owner, span)
            .into_iter()
            .map(|member| {
                CompletionItem::new(
                    member.name.clone(),
                    CompletionKind::from_member_kind(&member.kind),
                    member.span,
                )
            })
            .collect()
    }

    fn import_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let def = self.design.import_def_at(span)?;
        self.def_info(def)
    }

    fn expression_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let expr = self.expr_at(span)?;
        let resolution = self.design.expr_resolutions.get(&expr.id)?;
        match resolution {
            HirResolution::Def(id) => self.def_info(*id),
            HirResolution::Local(id) => {
                let local = self.design.locals.get(id.get())?;
                Some(DefinitionInfo::new(
                    local.name.clone(),
                    local.kind.into(),
                    local.span,
                ))
            }
            _ => None,
        }
    }

    fn member_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let owner = self.owner_at(span)?.id;
        let member = self.design.member_field_def_at(owner, span)?;
        Some(DefinitionInfo::with_doc(
            member.name.clone(),
            member.kind.label(),
            member.span,
            member.doc.clone(),
        ))
    }

    fn member_decl_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let member = self.design.member_decl_definition_at(span)?;
        Some(DefinitionInfo::with_doc(
            member.name.clone(),
            member.kind.label(),
            member.span,
            member.doc.clone(),
        ))
    }

    fn def_decl_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let def = self
            .design
            .defs
            .iter()
            .filter(|def| {
                def.span.source == span.source
                    && def.span.start <= span.start
                    && span.end <= def.span.end
            })
            .min_by_key(|def| def.span.end.saturating_sub(def.span.start))?;
        self.def_info(def.id)
    }

    fn type_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let owner = self.owner_at(span)?.id;
        let type_ref = self.design.type_ref_at(owner, span)?;
        if let Some(view) = self.design.view_def_for_type_ref(owner, &type_ref.ty, span) {
            return Some(DefinitionInfo::with_doc(
                view.name.clone(),
                view.kind.label(),
                view.span,
                view.doc.clone(),
            ));
        }
        let def = self.design.resolved_type_def_for_ref(type_ref)?;
        self.def_info(def)
    }

    fn def_info(&self, id: DefId) -> Option<DefinitionInfo> {
        let def = self.design.defs.get(id.get())?;
        Some(DefinitionInfo::with_doc(
            def.name.clone(),
            def.kind.into(),
            def.span,
            self.design.doc_for_item(id).map(ToOwned::to_owned),
        ))
    }

    fn completion_items_for_defs(
        &self,
        def_ids: impl IntoIterator<Item = DefId>,
        local_scope: Option<(DefId, Span)>,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        for id in def_ids {
            let Some(def) = self.design.defs.get(id.get()) else {
                continue;
            };
            items.push(CompletionItem::new(
                def.name.clone(),
                CompletionKind::from(def.kind),
                def.span,
            ));
        }
        if let Some((owner, cursor)) = local_scope {
            for local in &self.design.locals {
                if local.owner != owner || local.span.start > cursor.start {
                    continue;
                }
                items.push(CompletionItem::new(
                    local.name.clone(),
                    CompletionKind::from(local.kind),
                    local.span,
                ));
            }
        }
        items
    }

    fn owner_at(&self, span: Span) -> Option<&HirDef> {
        self.design
            .defs
            .iter()
            .filter(|def| {
                def.span.source == span.source
                    && def.span.start <= span.start
                    && span.end <= def.span.end
            })
            .min_by_key(|def| def.span.end.saturating_sub(def.span.start))
    }

    fn expr_at(&self, span: Span) -> Option<&HirExpr> {
        self.design
            .exprs
            .iter()
            .filter(|expr| {
                expr.span.source == span.source
                    && expr.span.start <= span.start
                    && span.end <= expr.span.end
            })
            .min_by_key(|expr| expr.span.end.saturating_sub(expr.span.start))
    }
}

impl fmt::Debug for HirAnalysis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HirAnalysis")
            .field("def_count", &self.design.defs.len())
            .field("local_count", &self.design.locals.len())
            .field("expr_count", &self.design.exprs.len())
            .finish()
    }
}

#[non_exhaustive]
pub struct TirAnalysis {
    design: TirDesign,
    facts: SemanticFacts,
}

impl TirAnalysis {
    fn new(design: TirDesign) -> Self {
        let facts = SemanticFacts::collect(&design);
        Self { design, facts }
    }

    pub fn design(&self) -> &TirDesign {
        &self.design
    }

    pub fn facts(&self) -> &SemanticFacts {
        &self.facts
    }

    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        self.facts.opaque_summaries()
    }

    pub fn debug_dump(&self) -> String {
        self.design.debug_dump()
    }

    pub fn expr_count(&self) -> usize {
        self.design.expr_phases().len()
    }

    pub fn binding_count(&self) -> usize {
        self.design.binding_kinds().len()
    }

    pub fn type_count(&self) -> usize {
        self.design.type_count()
    }

    pub fn hover_at(&self, span: Span) -> Option<HoverInfo> {
        let expr = self.expr_at(span)?;
        let phase = self
            .design
            .expr_phases()
            .get(&expr.id)
            .map(|phase| format!("{phase:?}"))
            .unwrap_or_else(|| "<unknown phase>".to_string());
        let ty = self.design.known_type_label(expr.id)?;
        let text = self.expr_binding_label(expr).map_or_else(
            || format!("{phase} {ty}"),
            |binding| format!("{phase} {ty} {binding}"),
        );
        Some(HoverInfo::new(expr.span, text))
    }

    fn expr_at(&self, span: Span) -> Option<&HirExpr> {
        self.design
            .hir()
            .exprs
            .iter()
            .filter(|expr| {
                expr.span.source == span.source
                    && expr.span.start <= span.start
                    && span.end <= expr.span.end
            })
            .min_by_key(|expr| expr.span.end.saturating_sub(expr.span.start))
    }

    fn expr_binding_label(&self, expr: &HirExpr) -> Option<String> {
        match self.design.hir().expr_resolutions.get(&expr.id)? {
            HirResolution::Def(id) => {
                let def = self.design.hir().defs.get(id.get())?;
                Some(format!("({} {})", <&'static str>::from(def.kind), def.name))
            }
            HirResolution::Local(id) => {
                let local = self.design.hir().locals.get(id.get())?;
                Some(format!(
                    "({} {})",
                    <&'static str>::from(local.kind),
                    local.name
                ))
            }
            _ => None,
        }
    }
}

impl fmt::Debug for TirAnalysis {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TirAnalysis")
            .field("expr_count", &self.expr_count())
            .field("binding_count", &self.binding_count())
            .field("type_count", &self.type_count())
            .finish()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DefinitionInfo {
    name: String,
    kind: &'static str,
    span: Span,
    doc: Option<String>,
}

impl DefinitionInfo {
    fn new(name: String, kind: &'static str, span: Span) -> Self {
        Self {
            name,
            kind,
            span,
            doc: None,
        }
    }

    fn with_doc(name: String, kind: &'static str, span: Span, doc: Option<String>) -> Self {
        Self {
            name,
            kind,
            span,
            doc,
        }
    }

    fn hover_text(&self) -> String {
        let signature = format!("{} {}", self.kind, self.name);
        self.doc
            .as_ref()
            .map(|doc| format!("{signature}\n\n{doc}"))
            .unwrap_or(signature)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> &'static str {
        self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn doc(&self) -> Option<&str> {
        self.doc.as_deref()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HoverInfo {
    span: Span,
    text: String,
}

impl HoverInfo {
    fn new(span: Span, text: String) -> Self {
        Self { span, text }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn text(&self) -> &str {
        &self.text
    }
}
