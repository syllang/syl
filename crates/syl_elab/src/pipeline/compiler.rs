use crate::CompileError;
use syl_hw::ParametricHwDesign;
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable, TirAnalysis};
use syl_span::Diagnostic;

use super::{ElaborationOutput, runner::TirStageRunner};

/// The top-level compiler for hardware elaboration.
///
/// Drives the pipeline from TIR through ConstMIR, MapIR, EIR,
/// DRC, and HW emission, producing a `ParametricHwDesign`.
#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HardwareCompiler {
    opaque_summaries: OpaqueSummaryTable,
}

impl HardwareCompiler {
    pub fn new() -> Self {
        Self {
            opaque_summaries: OpaqueSummaryTable::new(),
        }
    }

    /// Creates a compiler pre-loaded with opaque (external cell) summaries.
    pub fn with_opaque_summaries(opaque_summaries: OpaqueSummaryTable) -> Self {
        Self { opaque_summaries }
    }

    /// Registers a single opaque summary for an external cell definition.
    pub fn register_opaque_summary(&mut self, summary: OpaqueItemSummary) {
        self.opaque_summaries.register(summary);
    }

    /// Returns the current opaque summary table.
    pub fn opaque_summaries(&self) -> &OpaqueSummaryTable {
        &self.opaque_summaries
    }

    /// Compiles TIR analysis into a `ParametricHwDesign`.
    pub fn compile_tir(&self, tir: &TirAnalysis) -> Result<ParametricHwDesign, CompileError> {
        let cancellation = || false;
        Ok(self
            .compile_tir_with_token(tir, &cancellation)?
            .expect("non-cancelable compile_tir must not observe cancellation"))
    }

    /// Compiles TIR analysis into a `ParametricHwDesign` while honoring cooperative cancellation.
    ///
    /// Returns `Ok(None)` when cancellation is observed between pipeline stages.
    pub fn compile_tir_with_token<F: Fn() -> bool + ?Sized>(
        &self,
        tir: &TirAnalysis,
        cancellation: &F,
    ) -> Result<Option<ParametricHwDesign>, CompileError> {
        TirStageRunner::new(tir, &self.opaque_summaries, cancellation).compile_hwir()
    }

    /// Returns the full pipeline output (all stages) for a given TIR.
    pub fn output_for_tir(&self, tir: &TirAnalysis) -> ElaborationOutput {
        let cancellation = || false;
        TirStageRunner::new(tir, &self.opaque_summaries, &cancellation).stage_output()
    }

    /// Returns the full pipeline output while honoring cooperative cancellation.
    pub fn output_for_tir_with_token<F: Fn() -> bool + ?Sized>(
        &self,
        tir: &TirAnalysis,
        cancellation: &F,
    ) -> ElaborationOutput {
        TirStageRunner::new(tir, &self.opaque_summaries, cancellation).stage_output()
    }

    /// Returns diagnostics from the elaboration pipeline.
    pub fn diagnostics(&self, tir: &TirAnalysis) -> Vec<Diagnostic> {
        let cancellation = || false;
        TirStageRunner::new(tir, &self.opaque_summaries, &cancellation).diagnostics()
    }

    /// Returns diagnostics from the elaboration pipeline while honoring cooperative cancellation.
    pub fn diagnostics_with_token<F: Fn() -> bool + ?Sized>(
        &self,
        tir: &TirAnalysis,
        cancellation: &F,
    ) -> Vec<Diagnostic> {
        TirStageRunner::new(tir, &self.opaque_summaries, cancellation).diagnostics()
    }
}
