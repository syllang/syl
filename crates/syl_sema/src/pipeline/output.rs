use super::analysis::{HirAnalysis, TirAnalysis};
use crate::{facts::SemanticFacts, summary::opaque::OpaqueSummaryTable};
use syl_span::Diagnostic;

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
