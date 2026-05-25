use crate::{
    CompileError, HirResolver, StageOutput,
    completion::{CompletionItem, CompletionKind},
    facts::{ResolutionTable, SemanticFacts},
    hir::{HirDef, HirDesign, HirExpr},
    opaque_summary::OpaqueSummaryTable,
    tir::{TirDesign, TypePhaseChecker},
};
use std::{fmt, sync::Arc};
use syl_hir::{DefId, HirResolution};
use syl_span::{Diagnostic, Span};
use syl_syntax::AstFile;

#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticSourceFile<'files> {
    module_path: Vec<String>,
    ast: &'files AstFile,
}

impl<'files> SemanticSourceFile<'files> {
    pub fn new(module_path: Vec<String>, ast: &'files AstFile) -> Self {
        Self { module_path, ast }
    }

    pub fn module_path(&self) -> &[String] {
        &self.module_path
    }

    pub fn ast(&self) -> &'files AstFile {
        self.ast
    }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct SemanticCompiler;

impl SemanticCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn session<'files>(&self, files: &'files [AstFile]) -> SemanticSession<'files> {
        SemanticSession::new(files)
    }

    pub fn session_sources<'files>(
        &self,
        sources: Vec<SemanticSourceFile<'files>>,
    ) -> SemanticSession<'files> {
        SemanticSession::new_sources(sources)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticSession<'files> {
    sources: Vec<SemanticSourceFile<'files>>,
}

impl<'files> SemanticSession<'files> {
    pub fn new(files: &'files [AstFile]) -> Self {
        let sources = files
            .iter()
            .enumerate()
            .map(|(index, ast)| SemanticSourceFile::new(vec![format!("file{index}")], ast))
            .collect();
        Self { sources }
    }

    pub fn new_sources(sources: Vec<SemanticSourceFile<'files>>) -> Self {
        Self { sources }
    }

    pub fn resolve_hir(&self) -> Result<HirAnalysis, CompileError> {
        HirResolver::new_sources(self.semantic_sources())
            .resolve()
            .map(HirAnalysis::new)
    }

    pub fn resolve_hir_partial(&self) -> HirAnalysisOutput {
        let (design, errors) = HirResolver::new_sources(self.semantic_sources()).resolve_partial();
        let diagnostics = errors.into_iter().map(Diagnostic::from).collect();
        HirAnalysisOutput::new(HirAnalysis::new(design), diagnostics)
    }

    pub fn check(&self) -> SemanticOutput {
        let hir = match self.resolve_hir_collect() {
            Ok(hir) => hir,
            Err(errors) => {
                return SemanticOutput::new(
                    None,
                    errors.into_iter().map(Diagnostic::from).collect(),
                );
            }
        };
        let tir = hir.check_tir_partial();
        let diagnostics = tir.diagnostics().to_vec();
        SemanticOutput::new(tir.into_stage(), diagnostics)
    }

    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        self.check().diagnostics().to_vec()
    }

    fn resolve_hir_collect(&self) -> Result<HirAnalysis, Vec<CompileError>> {
        HirResolver::new_sources(self.semantic_sources())
            .resolve_collect()
            .map(HirAnalysis::new)
    }

    fn semantic_sources(&self) -> Vec<SemanticSourceFile<'files>> {
        self.sources
            .iter()
            .map(|source| SemanticSourceFile::new(source.module_path().to_vec(), source.ast()))
            .collect()
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticOutput {
    tir: Option<TirAnalysis>,
    diagnostics: Vec<Diagnostic>,
}

impl SemanticOutput {
    fn new(tir: Option<TirAnalysis>, diagnostics: Vec<Diagnostic>) -> Self {
        Self { tir, diagnostics }
    }

    pub fn tir(&self) -> Option<&TirAnalysis> {
        self.tir.as_ref()
    }

    pub fn facts(&self) -> Option<&SemanticFacts> {
        self.tir().map(TirAnalysis::facts)
    }

    pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {
        self.tir().map(TirAnalysis::opaque_summaries)
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct HirAnalysisOutput {
    output: StageOutput<HirAnalysis>,
}

impl HirAnalysisOutput {
    fn new(stage: HirAnalysis, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            output: StageOutput::new(Some(stage), diagnostics),
        }
    }

    pub fn stage(&self) -> &HirAnalysis {
        self.output
            .stage()
            .expect("HIR analysis output is always constructed with a resolved stage")
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.output.diagnostics()
    }
}

#[non_exhaustive]
pub struct HirAnalysis {
    design: Arc<HirDesign>,
    resolution: ResolutionTable,
}

impl HirAnalysis {
    fn new(design: HirDesign) -> Self {
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
        self.type_definition_at(span)
    }

    pub fn hover_at(&self, span: Span) -> Option<HoverInfo> {
        let definition = self.definition_at(span)?;
        Some(HoverInfo::new(
            definition.span,
            format!("{} {}", definition.kind, definition.name),
        ))
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
        Some(DefinitionInfo::new(
            member.name.clone(),
            member.kind.label(),
            member.span,
        ))
    }

    fn member_decl_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let member = self.design.member_decl_definition_at(span)?;
        Some(DefinitionInfo::new(
            member.name.clone(),
            member.kind.label(),
            member.span,
        ))
    }

    fn type_definition_at(&self, span: Span) -> Option<DefinitionInfo> {
        let owner = self.owner_at(span)?.id;
        let type_ref = self.design.type_ref_at(owner, span)?;
        if let Some(view) = self.design.view_def_for_type_ref(owner, &type_ref.ty, span) {
            return Some(DefinitionInfo::new(
                view.name.clone(),
                view.kind.label(),
                view.span,
            ));
        }
        let def = self.design.resolved_type_def_for_ref(type_ref)?;
        self.def_info(def)
    }

    fn def_info(&self, id: DefId) -> Option<DefinitionInfo> {
        let def = self.design.defs.get(id.get())?;
        Some(DefinitionInfo::new(
            def.name.clone(),
            def.kind.into(),
            def.span,
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
}

impl TirAnalysis {
    fn new(design: TirDesign) -> Self {
        Self { design }
    }

    pub fn design(&self) -> &TirDesign {
        &self.design
    }

    pub fn facts(&self) -> &SemanticFacts {
        self.design.facts()
    }

    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        self.facts().opaque_summaries()
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
}

impl DefinitionInfo {
    fn new(name: String, kind: &'static str, span: Span) -> Self {
        Self { name, kind, span }
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
