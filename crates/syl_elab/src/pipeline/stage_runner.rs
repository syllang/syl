use super::{
    ConstMirStage, DrcStage, DriverFactsStage, EirBuildStage, EirFactsStage, EirStage,
    EirValidationStage, ElabStage, ElaborationOutput, MapIrStage,
};
use crate::{
    CompileError,
    const_mir::ConstMirBuilder,
    driver::{DriverDrcChecker, DriverFactsCollector},
    eir::{EirDesignComposer, EirFactCollector, EirValidator},
    hardware_metadata::HardwareMetadata,
    hardware_metadata_lower::HardwareMetadataLowerer,
    hw_lower::HwLowerer,
    map_ir::MapIrBuilder,
};
use syl_hw::ParametricHwDesign;
use syl_sema::{OpaqueSummaryTable, TirAnalysis};
use syl_span::Diagnostic;

#[non_exhaustive]
pub(super) struct TirStageRunner<'tir_stage> {
    tir: &'tir_stage TirAnalysis,
    opaque_summaries: &'tir_stage OpaqueSummaryTable,
}

impl<'tir_stage> TirStageRunner<'tir_stage> {
    pub(super) fn new(
        tir: &'tir_stage TirAnalysis,
        opaque_summaries: &'tir_stage OpaqueSummaryTable,
    ) -> Self {
        Self {
            tir,
            opaque_summaries,
        }
    }

    pub(super) fn compile_hwir(&self) -> Result<ParametricHwDesign, CompileError> {
        let opaque_summaries = self
            .tir
            .facts()
            .opaque_summaries()
            .merged(self.opaque_summaries);
        let const_mir = ConstMirPass::run(self.tir)?;
        let map_ir = MapIrPass::run(self.tir)?;
        let eir_build = EirBuildPass::run(self.tir, &const_mir, &map_ir)?;
        let _validation = EirValidationPass::run(&eir_build)?;
        let eir_facts = EirFactsPass::run(&eir_build, &opaque_summaries)?;
        let eir = EirComposePass::run(&eir_build, &eir_facts);
        let driver_facts = DriverFactsPass::run(&eir).map_err(first_error)?;
        let _drc = DrcPass::run(&eir, &driver_facts).map_err(first_error)?;
        HwLoweringPass::run(&eir)
    }

    pub(super) fn diagnostics(&self) -> Vec<Diagnostic> {
        self.stage_output().into_diagnostics()
    }

    pub(super) fn stage_output(&self) -> ElaborationOutput {
        let opaque_summaries = self
            .tir
            .facts()
            .opaque_summaries()
            .merged(self.opaque_summaries);
        let finish = |const_mir: Option<ConstMirStage>,
                      map_ir: Option<MapIrStage>,
                      eir_build: Option<EirBuildStage>,
                      eir_validation: Option<EirValidationStage>,
                      eir_facts: Option<EirFactsStage>,
                      eir: Option<EirStage>,
                      driver_facts: Option<DriverFactsStage>,
                      drc: Option<DrcStage>,
                      metadata: Option<HardwareMetadata>,
                      hwir: Option<ParametricHwDesign>,
                      diagnostics: Vec<Diagnostic>| ElaborationOutput {
            const_mir,
            map_ir,
            eir_build,
            eir_validation,
            eir_facts,
            eir,
            driver_facts,
            drc,
            metadata,
            hwir,
            diagnostics,
        };
        let mut diagnostics = Vec::new();

        let const_mir = match ConstMirPass::run(self.tir) {
            Ok(stage) => Some(stage),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                return finish(
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let map_ir = match MapIrPass::run(self.tir) {
            Ok(stage) => Some(stage),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                return finish(
                    const_mir,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let eir_build = match EirBuildPass::run(
            self.tir,
            const_mir
                .as_ref()
                .expect("const mir pass succeeded before EIR build"),
            map_ir
                .as_ref()
                .expect("map ir pass succeeded before EIR build"),
        ) {
            Ok(stage) => Some(stage),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                return finish(
                    const_mir,
                    map_ir,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let eir_validation = match EirValidationPass::run(
            eir_build
                .as_ref()
                .expect("EIR build stage must exist before validation"),
        ) {
            Ok(stage) => Some(stage),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                return finish(
                    const_mir,
                    map_ir,
                    eir_build,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let eir_facts = match EirFactsPass::run(
            eir_build
                .as_ref()
                .expect("EIR build stage must exist before facts collection"),
            &opaque_summaries,
        ) {
            Ok(stage) => Some(stage),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                return finish(
                    const_mir,
                    map_ir,
                    eir_build,
                    eir_validation,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let eir = Some(EirComposePass::run(
            eir_build.as_ref().expect("EIR build stage must exist"),
            eir_facts.as_ref().expect("EIR facts stage must exist"),
        ));
        let driver_facts = match DriverFactsPass::run(eir.as_ref().expect("EIR stage must exist")) {
            Ok(stage) => Some(stage),
            Err(errors) => {
                diagnostics.extend(errors.into_iter().map(Diagnostic::from));
                return finish(
                    const_mir,
                    map_ir,
                    eir_build,
                    eir_validation,
                    eir_facts,
                    eir,
                    None,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let drc = match DrcPass::run(
            eir.as_ref().expect("EIR stage must exist"),
            driver_facts
                .as_ref()
                .expect("driver facts stage must exist"),
        ) {
            Ok(stage) => Some(stage),
            Err(errors) => {
                diagnostics.extend(errors.into_iter().map(Diagnostic::from));
                return finish(
                    const_mir,
                    map_ir,
                    eir_build,
                    eir_validation,
                    eir_facts,
                    eir,
                    driver_facts,
                    None,
                    None,
                    None,
                    diagnostics,
                );
            }
        };
        let metadata = match HardwareMetadataPass::run(
            driver_facts
                .as_ref()
                .expect("driver facts stage must exist before metadata lowering"),
            &opaque_summaries,
        ) {
            Ok(metadata) => Some(metadata),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                None
            }
        };
        let hwir = match HwLoweringPass::run(eir.as_ref().expect("EIR stage must exist")) {
            Ok(hwir) => Some(hwir),
            Err(error) => {
                diagnostics.push(Diagnostic::from(error));
                None
            }
        };

        finish(
            const_mir,
            map_ir,
            eir_build,
            eir_validation,
            eir_facts,
            eir,
            driver_facts,
            drc,
            metadata,
            hwir,
            diagnostics,
        )
    }
}

#[non_exhaustive]
struct ConstMirPass;

impl ConstMirPass {
    fn run(tir: &TirAnalysis) -> Result<ConstMirStage, CompileError> {
        ConstMirBuilder::new(tir.design())
            .build()
            .map(ConstMirStage::new)
    }
}

#[non_exhaustive]
struct MapIrPass;

impl MapIrPass {
    fn run(tir: &TirAnalysis) -> Result<MapIrStage, CompileError> {
        MapIrBuilder::new(tir.design()).build().map(MapIrStage::new)
    }
}

#[non_exhaustive]
struct EirBuildPass;

impl EirBuildPass {
    fn run(
        tir: &TirAnalysis,
        const_mir: &ConstMirStage,
        map_ir: &MapIrStage,
    ) -> Result<EirBuildStage, CompileError> {
        ElabStage::from_tir(tir.design()).build_raw_eir(const_mir, map_ir)
    }
}

#[non_exhaustive]
struct EirValidationPass;

impl EirValidationPass {
    fn run(eir_build: &EirBuildStage) -> Result<EirValidationStage, CompileError> {
        EirValidator::new(eir_build.design.modules()).validate()?;
        Ok(EirValidationStage::new(eir_build.design.modules().len()))
    }
}

#[non_exhaustive]
struct EirFactsPass;

impl EirFactsPass {
    fn run(
        eir_build: &EirBuildStage,
        opaque_summaries: &OpaqueSummaryTable,
    ) -> Result<EirFactsStage, CompileError> {
        EirFactCollector::collect(eir_build.design.modules(), opaque_summaries)
            .map(EirFactsStage::new)
    }
}

#[non_exhaustive]
struct EirComposePass;

impl EirComposePass {
    fn run(eir_build: &EirBuildStage, eir_facts: &EirFactsStage) -> EirStage {
        EirStage::new(EirDesignComposer::compose(
            eir_build.design.clone(),
            eir_facts.facts.clone(),
        ))
    }
}

#[non_exhaustive]
struct DriverFactsPass;

impl DriverFactsPass {
    fn run(eir: &EirStage) -> Result<DriverFactsStage, Vec<CompileError>> {
        DriverFactsCollector::new(&eir.design)
            .collect()
            .map(DriverFactsStage::new)
    }
}

#[non_exhaustive]
struct DrcPass;

impl DrcPass {
    fn run(eir: &EirStage, driver_facts: &DriverFactsStage) -> Result<DrcStage, Vec<CompileError>> {
        DriverDrcChecker::new(&eir.design, &driver_facts.facts)
            .check_collect()
            .map(DrcStage::new)
    }
}

#[non_exhaustive]
struct HardwareMetadataPass;

impl HardwareMetadataPass {
    fn run(
        driver_facts: &DriverFactsStage,
        opaque_summaries: &OpaqueSummaryTable,
    ) -> Result<HardwareMetadata, CompileError> {
        HardwareMetadataLowerer::new(&driver_facts.facts).lower(opaque_summaries)
    }
}

#[non_exhaustive]
struct HwLoweringPass;

impl HwLoweringPass {
    fn run(eir: &EirStage) -> Result<ParametricHwDesign, CompileError> {
        HwLowerer::new(&eir.design).lower()
    }
}

fn first_error(mut errors: Vec<CompileError>) -> CompileError {
    errors.remove(0)
}
