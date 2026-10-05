//! Per-module stage entry points. The design query composes their EIR.

use super::{
    ConstMirPass, DrcPass, DriverFactsPass, EirBuildPass, EirComposePass, EirFactsPass,
    EirValidationPass, HardwareMetadataPass, HwLoweringPass, MapIrPass,
};
use crate::CompileError;
use crate::eir::EirRawDesign;
use crate::pipeline::{ConstMirStage, EirBuildStage, ElaborationOutput, MapIrStage};
use syl_sema::{OpaqueSummaryTable, TirAnalysis};
use syl_span::Diagnostic;

pub(crate) fn const_mir_stage(tir: &TirAnalysis) -> Result<ConstMirStage, CompileError> {
    ConstMirPass::run(tir)
}

pub(crate) fn map_ir_stage(tir: &TirAnalysis) -> Result<MapIrStage, CompileError> {
    MapIrPass::run(tir)
}

/// Elaborates one owner. Companion cells used only as stubs are omitted.
///
/// Validation runs on the retained module. Stub modules exist so const
/// evaluation and `place` can see signatures; they are not this owner's output.
pub(crate) fn module_eir_stage(
    tir: &TirAnalysis,
    const_mir: &ConstMirStage,
    map_ir: &MapIrStage,
    module_name: &str,
) -> Result<EirBuildStage, CompileError> {
    let stage = EirBuildPass::run(tir, const_mir, map_ir)?;
    let stage = EirBuildStage::new(stage.into_raw().retain_named(module_name));
    let _ = EirValidationPass::run(&stage)?;
    Ok(stage)
}

pub(crate) fn output_from_modules<C: Fn() -> bool + ?Sized>(
    opaque_summaries: &OpaqueSummaryTable,
    modules: &[&EirBuildStage],
    cancellation: &C,
) -> ElaborationOutput {
    let raw = EirRawDesign::concat(
        modules
            .iter()
            .map(|stage| EirRawDesign::new(stage.design.modules().to_vec())),
    );
    let eir_build = EirBuildStage::new(raw);
    let mut diagnostics = Vec::new();
    let none = || ElaborationOutput {
        const_mir: None,
        map_ir: None,
        eir_build: None,
        eir_validation: None,
        eir_facts: None,
        eir: None,
        driver_facts: None,
        drc: None,
        metadata: None,
        hwir: None,
        diagnostics: diagnostics.clone(),
    };
    if cancellation() {
        return none();
    }
    let eir_validation = match EirValidationPass::run(&eir_build) {
        Ok(stage) => Some(stage),
        Err(error) => {
            diagnostics.push(Diagnostic::from(error));
            return ElaborationOutput {
                eir_build: Some(eir_build),
                diagnostics,
                ..empty_output()
            };
        }
    };
    if cancellation() {
        return ElaborationOutput {
            eir_build: Some(eir_build),
            eir_validation,
            diagnostics,
            ..empty_output()
        };
    }
    let eir_facts = match EirFactsPass::run(&eir_build, opaque_summaries) {
        Ok(stage) => Some(stage),
        Err(error) => {
            diagnostics.push(Diagnostic::from(error));
            return ElaborationOutput {
                eir_build: Some(eir_build),
                eir_validation,
                diagnostics,
                ..empty_output()
            };
        }
    };
    let eir = Some(EirComposePass::run(
        &eir_build,
        eir_facts.as_ref().expect("facts stage exists"),
    ));
    if cancellation() {
        return ElaborationOutput {
            eir_build: Some(eir_build),
            eir_validation,
            eir_facts,
            eir,
            diagnostics,
            ..empty_output()
        };
    }
    let driver_facts = match DriverFactsPass::run(eir.as_ref().expect("eir"), opaque_summaries) {
        Ok(stage) => Some(stage),
        Err(errors) => {
            diagnostics.extend(errors.into_iter().map(Diagnostic::from));
            return ElaborationOutput {
                eir_build: Some(eir_build),
                eir_validation,
                eir_facts,
                eir,
                diagnostics,
                ..empty_output()
            };
        }
    };
    if cancellation() {
        return ElaborationOutput {
            eir_build: Some(eir_build),
            eir_validation,
            eir_facts,
            eir,
            driver_facts,
            diagnostics,
            ..empty_output()
        };
    }
    let drc = match DrcPass::run(
        eir.as_ref().expect("eir"),
        driver_facts.as_ref().expect("driver facts"),
    ) {
        Ok(stage) => Some(stage),
        Err(errors) => {
            diagnostics.extend(errors.into_iter().map(Diagnostic::from));
            return ElaborationOutput {
                eir_build: Some(eir_build),
                eir_validation,
                eir_facts,
                eir,
                driver_facts,
                diagnostics,
                ..empty_output()
            };
        }
    };
    if cancellation() {
        return ElaborationOutput {
            eir_build: Some(eir_build),
            eir_validation,
            eir_facts,
            eir,
            driver_facts,
            drc,
            diagnostics,
            ..empty_output()
        };
    }
    let metadata = match HardwareMetadataPass::run(
        driver_facts.as_ref().expect("driver facts"),
        opaque_summaries,
    ) {
        Ok(metadata) => Some(metadata),
        Err(error) => {
            diagnostics.push(Diagnostic::from(error));
            None
        }
    };
    if cancellation() {
        return ElaborationOutput {
            eir_build: Some(eir_build),
            eir_validation,
            eir_facts,
            eir,
            driver_facts,
            drc,
            metadata,
            diagnostics,
            ..empty_output()
        };
    }
    let hwir = match HwLoweringPass::run(eir.as_ref().expect("eir")) {
        Ok(hwir) => Some(hwir),
        Err(error) => {
            diagnostics.push(Diagnostic::from(error));
            None
        }
    };
    ElaborationOutput {
        const_mir: None,
        map_ir: None,
        eir_build: Some(eir_build),
        eir_validation,
        eir_facts,
        eir,
        driver_facts,
        drc,
        metadata,
        hwir,
        diagnostics,
    }
}

pub(crate) fn diagnostics_output(diagnostics: Vec<Diagnostic>) -> ElaborationOutput {
    ElaborationOutput {
        diagnostics,
        ..empty_output()
    }
}

fn empty_output() -> ElaborationOutput {
    ElaborationOutput {
        const_mir: None,
        map_ir: None,
        eir_build: None,
        eir_validation: None,
        eir_facts: None,
        eir: None,
        driver_facts: None,
        drc: None,
        metadata: None,
        hwir: None,
        diagnostics: Vec::new(),
    }
}
