use crate::{
    CompileError,
    completion::{CompletionItem, CompletionKind},
    const_mir::{ConstMirBuilder, ConstMirProgram},
    driver::{DriverAnalyzer, DriverFacts},
    eir::EirDesign,
    hw_lower::HwLowerer,
    map_ir::{MapIrBuilder, MapIrProgram},
    source::{HirDesign, HirResolver},
    tir::{TirDesign, TypePhaseChecker},
};
use std::sync::Arc;
use syl_hir::{DefId, HirResolution};
use syl_hw::ParametricHwDesign;
use syl_sema::StageOutput;
use syl_span::{Diagnostic, Span};
use syl_syntax::AstFile;
mod stage;
mod stage_runner;
pub use stage::ElabStage;
use stage_runner::{HirStageRunner, StageRunner, TirStageRunner};
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct MiddleCompiler {
    options: MiddleOptions,
}
#[derive(Debug, Default)]
struct MiddleOptions {
    collect_metadata: MetadataMode,
}
#[derive(Debug, Default)]
enum MetadataMode {
    #[default]
    DriverFacts,
}

impl MiddleCompiler {
    pub fn new() -> Self {
        Self {
            options: MiddleOptions::default(),
        }
    }
    pub fn compile_files(&self, files: &[AstFile]) -> Result<ParametricHwDesign, CompileError> {
        match self.options.collect_metadata {
            MetadataMode::DriverFacts => {}
        }
        self.session(files).compile_hwir()
    }
    pub fn session<'files>(&self, files: &'files [AstFile]) -> MiddleSession<'files> {
        MiddleSession::new(files)
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct MiddleSession<'files> {
    files: &'files [AstFile],
}

impl<'files> MiddleSession<'files> {
    pub fn new(files: &'files [AstFile]) -> Self {
        Self { files }
    }
    pub fn resolve_hir(&self) -> Result<HirStage, CompileError> {
        HirResolver::new(self.files).resolve().map(HirStage::new)
    }
    pub fn resolve_hir_partial(&self) -> HirStageOutput {
        let (design, errors) = HirResolver::new(self.files).resolve_partial();
        let diagnostics = errors
            .into_iter()
            .map(|error| self.diagnostic_for_error(error))
            .collect();
        HirStageOutput::new(HirStage::new(design), diagnostics)
    }
    fn resolve_hir_collect(&self) -> Result<HirStage, Vec<CompileError>> {
        HirResolver::new(self.files)
            .resolve_collect()
            .map(HirStage::new)
    }
    pub fn compile_hwir(&self) -> Result<ParametricHwDesign, CompileError> {
        StageRunner::new(self).compile_hwir()
    }
    pub fn check(&self) -> MiddleOutput {
        let diagnostics = self.diagnostics();
        if !diagnostics.is_empty() {
            return MiddleOutput::new(None, diagnostics);
        }
        match self.compile_hwir() {
            Ok(hwir) => MiddleOutput::new(Some(hwir), Vec::new()),
            Err(error) => MiddleOutput::new(None, vec![self.diagnostic_for_error(error)]),
        }
    }
    pub fn diagnostics(&self) -> Vec<Diagnostic> {
        StageRunner::new(self).diagnostics()
    }

    fn diagnostic_for_error(&self, error: CompileError) -> Diagnostic {
        Diagnostic::from(error)
    }
}
#[non_exhaustive]
pub struct MiddleOutput {
    hwir: Option<ParametricHwDesign>,
    diagnostics: Vec<Diagnostic>,
}

impl MiddleOutput {
    fn new(hwir: Option<ParametricHwDesign>, diagnostics: Vec<Diagnostic>) -> Self {
        Self { hwir, diagnostics }
    }

    pub fn hwir(&self) -> Option<&ParametricHwDesign> {
        self.hwir.as_ref()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
}
#[non_exhaustive]
pub struct HirStageOutput {
    output: StageOutput<HirStage>,
}

impl HirStageOutput {
    fn new(stage: HirStage, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            output: StageOutput::new(Some(stage), diagnostics),
        }
    }

    pub fn stage(&self) -> &HirStage {
        self.output
            .stage()
            .expect("HIR stage output is always constructed with a resolved stage")
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        self.output.diagnostics()
    }

    pub fn semantic_diagnostics(&self) -> Vec<Diagnostic> {
        if !self.output.diagnostics().is_empty() {
            return self.output.diagnostics().to_vec();
        }
        self.stage().downstream_diagnostics()
    }
}
#[non_exhaustive]
pub struct HirStage {
    design: Arc<HirDesign>,
}

impl HirStage {
    fn new(design: HirDesign) -> Self {
        Self {
            design: Arc::new(design),
        }
    }

    pub fn def_count(&self) -> usize {
        self.design.defs.len()
    }

    pub fn local_count(&self) -> usize {
        self.design.locals.len()
    }

    pub fn check_tir(&self) -> Result<TirStage, CompileError> {
        TypePhaseChecker::new(Arc::clone(&self.design))
            .check()
            .map(TirStage::new)
    }

    pub fn check_tir_partial(&self) -> StageOutput<TirStage> {
        TypePhaseChecker::new(Arc::clone(&self.design))
            .check_output()
            .map_stage(TirStage::new)
    }

    pub fn downstream_diagnostics(&self) -> Vec<Diagnostic> {
        HirStageRunner::new(self).diagnostics()
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

    fn owner_at(&self, span: Span) -> Option<&crate::source::HirDef> {
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

    fn expr_at(&self, span: Span) -> Option<&crate::source::HirExpr> {
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

#[non_exhaustive]
pub struct TirStage {
    design: TirDesign,
}

impl TirStage {
    fn new(design: TirDesign) -> Self {
        Self { design }
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

    pub fn downstream_diagnostics(&self) -> Vec<Diagnostic> {
        self.downstream_output().into_diagnostics()
    }

    pub fn downstream_output(&self) -> TirStageOutput {
        TirStageRunner::new(self).stage_output()
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

    pub fn build_const_mir(&self) -> Result<ConstMirStage, CompileError> {
        ConstMirBuilder::new(&self.design)
            .build()
            .map(ConstMirStage::new)
    }

    pub fn build_map_ir(&self) -> Result<MapIrStage, CompileError> {
        MapIrBuilder::new(&self.design).build().map(MapIrStage::new)
    }

    pub fn build_program(&self) -> ElabStage {
        ElabStage::from_tir(&self.design)
    }

    fn expr_at(&self, span: Span) -> Option<&crate::source::HirExpr> {
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

    fn expr_binding_label(&self, expr: &crate::source::HirExpr) -> Option<String> {
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

#[non_exhaustive]
pub struct TirStageOutput {
    const_mir: Option<ConstMirStage>,
    map_ir: Option<MapIrStage>,
    eir: Option<EirStage>,
    drivers: Option<DriverStage>,
    hwir: Option<ParametricHwDesign>,
    diagnostics: Vec<Diagnostic>,
}

impl TirStageOutput {
    pub fn const_mir(&self) -> Option<&ConstMirStage> {
        self.const_mir.as_ref()
    }

    pub fn map_ir(&self) -> Option<&MapIrStage> {
        self.map_ir.as_ref()
    }

    pub fn eir(&self) -> Option<&EirStage> {
        self.eir.as_ref()
    }

    pub fn drivers(&self) -> Option<&DriverStage> {
        self.drivers.as_ref()
    }

    pub fn hwir(&self) -> Option<&ParametricHwDesign> {
        self.hwir.as_ref()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }
}

#[non_exhaustive]
pub struct ConstMirStage {
    program: ConstMirProgram,
}

impl ConstMirStage {
    fn new(program: ConstMirProgram) -> Self {
        Self { program }
    }

    pub fn node_count(&self) -> usize {
        self.program.node_count()
    }

    pub fn local_ref_count(&self) -> usize {
        self.program.local_ref_count()
    }

    pub fn resolved_local_ref_count(&self) -> usize {
        self.program.resolved_local_ref_count()
    }
}

#[non_exhaustive]
pub struct MapIrStage {
    program: MapIrProgram,
}

impl MapIrStage {
    fn new(program: MapIrProgram) -> Self {
        Self { program }
    }

    pub fn map_count(&self) -> usize {
        self.program.len()
    }

    pub fn param_count(&self) -> usize {
        self.program.param_count()
    }

    pub fn resolved_param_count(&self) -> usize {
        self.program.resolved_param_count()
    }

    pub fn local_ref_count(&self) -> usize {
        self.program.local_ref_count()
    }

    pub fn resolved_local_ref_count(&self) -> usize {
        self.program.resolved_local_ref_count()
    }
}

#[non_exhaustive]
pub struct EirStage {
    design: EirDesign,
}

impl EirStage {
    fn new(design: EirDesign) -> Self {
        Self { design }
    }

    pub fn module_count(&self) -> usize {
        self.design.modules().len()
    }

    pub fn drive_count(&self) -> usize {
        self.design.drives().len()
    }

    pub fn analyze_drivers(&self) -> Result<DriverStage, CompileError> {
        DriverAnalyzer::new(&self.design)
            .analyze()
            .map(DriverStage::new)
    }

    pub fn analyze_drivers_collect(&self) -> Result<DriverStage, Vec<CompileError>> {
        DriverAnalyzer::new(&self.design)
            .analyze_collect()
            .map(DriverStage::new)
    }
}

#[non_exhaustive]
pub struct DriverStage {
    facts: DriverFacts,
}

impl DriverStage {
    fn new(facts: DriverFacts) -> Self {
        Self { facts }
    }

    pub fn drive_count(&self) -> usize {
        self.facts.drives().len()
    }

    pub fn read_count(&self) -> usize {
        self.facts.reads().len()
    }

    pub fn create_count(&self) -> usize {
        self.facts.creates().len()
    }

    pub fn lower_hwir(&self, eir: &EirStage) -> Result<ParametricHwDesign, CompileError> {
        HwLowerer::new(&eir.design, &self.facts).lower()
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

#[cfg(test)]
mod tests;
