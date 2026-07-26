use crate::CompileError;
use syl_hw::ParametricHwDesign;
use syl_sema::{OpaqueItemSummary, OpaqueSummaryTable, TirAnalysis};
use syl_span::Diagnostic;

use super::{ElaborationOutput, runner::TirStageRunner};

/// Top-level entry for hardware elaboration from typed IR.
///
/// Holds optional opaque (external cell) summaries, then drives TIR through
/// ConstMIR → MapIR → EIR → driver facts / DRC → HWIR lowering.
///
/// # Main usage flow
///
/// ```text
///  TirAnalysis  (+ optional OpaqueSummaryTable)
///              |
///              v
///    +---------------------+
///    |  HardwareCompiler   |   new()
///    |                     |   with_opaque_summaries(...)
///    |                     |   register_opaque_summary(...)
///    +----------+----------+
///               |
///     +---------+---------------------------+
///     |                                     |
///     v                                     v
///  compile_tir[ _with_token ]      output_for_tir[ _with_token ]
///  Result<ParametricHwDesign, E>   ElaborationOutput
///  (HWIR only; fail-fast)          (all stages + diagnostics)
///     |                                     |
///     |                          +----------+-----------+
///     |                          |          |           |
///     |                          v          v           v
///     |                     const_mir()  eir() /    hwir()
///     |                     map_ir()     drc() /    diagnostics()
///     |                                  metadata()
///     v
///  ParametricHwDesign
///              |
///              v
///  SystemVerilogBackend::emit  /  HwNormalizer::normalize
/// ```
///
/// Typical paths:
/// - **Strict HWIR**: `compile_tir(&tir)?` then emit.
/// - **Stage inspection / IDE**: `output_for_tir(&tir)` and read partial stages
///   even when later stages did not complete.
/// - **Cancellation**: `*_with_token` variants return `Ok(None)` / partial
///   output when cooperative cancellation is observed between stages.
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
