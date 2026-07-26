use super::analysis::{HirAnalysis, TirAnalysis};
use crate::{facts::SemanticFacts, summary::opaque::OpaqueSummaryTable};
use syl_span::Diagnostic;

/// Partial stage result: optional successful value plus accumulated diagnostics.
///
/// Used by fail-soft paths such as [`HirAnalysis::check_tir_partial`](super::HirAnalysis::check_tir_partial).
///
/// # Main usage flow
///
/// ```text
///  some_stage_partial()
///         |
///         v
///    StageOutput<T>
///         |
///     +---+------------------+
///     |                      |
///     v                      v
///  stage() /              diagnostics()
///  into_stage()
///  partial_stage()
/// ```
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StageOutput<T> {
    stage: Option<T>,
    diagnostics: Vec<Diagnostic>,
}

impl<T> StageOutput<T> {
    pub fn new(stage: Option<T>, diagnostics: Vec<Diagnostic>) -> Self {
        Self { stage, diagnostics }
    }

    pub fn stage(&self) -> Option<&T> {
        self.stage.as_ref()
    }

    pub fn partial_stage(&self) -> Option<&T> {
        self.stage()
    }

    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }

    pub fn into_stage(self) -> Option<T> {
        self.stage
    }

    pub fn into_diagnostics(self) -> Vec<Diagnostic> {
        self.diagnostics
    }

    pub fn into_parts(self) -> (Option<T>, Vec<Diagnostic>) {
        (self.stage, self.diagnostics)
    }

    pub fn map_stage<U>(self, map: impl FnOnce(T) -> U) -> StageOutput<U> {
        StageOutput::new(self.stage.map(map), self.diagnostics)
    }
}

/// One-shot semantic pipeline result: optional TIR plus accumulated diagnostics.
///
/// Produced by [`SemanticSession::check`](super::SemanticSession::check). Prefer
/// this when callers want HIR+TIR in one call and do not need an intermediate
/// [`HirAnalysis`](super::HirAnalysis) handle.
///
/// # Main usage flow
///
/// ```text
///  SemanticSession::check()
///              |
///              v
///    +---------------------+
///    |   SemanticOutput    |
///    +----------+----------+
///               |
///     +---------+----------+------------------+
///     |                    |                  |
///     v                    v                  v
///   tir()              facts() /          diagnostics()
///   Option<Tir>        opaque_summaries()
///     |
///     v
///  HardwareCompiler  (when tir is Some)
/// ```
///
/// If HIR resolution failed, `tir()` is `None` and only HIR diagnostics are set.
#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticOutput {
    tir: Option<TirAnalysis>,
    diagnostics: Vec<Diagnostic>,
}

impl SemanticOutput {
    pub(super) fn new(tir: Option<TirAnalysis>, diagnostics: Vec<Diagnostic>) -> Self {
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

/// HIR resolution result with diagnostics, always carrying a stage handle.
///
/// Produced by [`SemanticSession::resolve_hir_partial`](super::SemanticSession::resolve_hir_partial).
/// Use when IDE/query code must keep working against partial HIR under errors.
///
/// # Main usage flow
///
/// ```text
///  SemanticSession::resolve_hir_partial()
///              |
///              v
///    +---------------------+
///    |  HirAnalysisOutput  |
///    +----------+----------+
///               |
///     +---------+----------+
///     |                    |
///     v                    v
///  stage()             diagnostics()
///  &HirAnalysis
///     |
///     +---> definition_at / hover_at / check_tir_partial ...
/// ```
#[derive(Debug)]
#[non_exhaustive]
pub struct HirAnalysisOutput {
    output: StageOutput<HirAnalysis>,
}

impl HirAnalysisOutput {
    pub(super) fn new(stage: HirAnalysis, diagnostics: Vec<Diagnostic>) -> Self {
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
