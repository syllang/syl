use super::{ConstMirStage, DriverStage, EirStage, ElabStage, ElaborationOutput, MapIrStage};
use crate::{
    CompileError, const_mir::ConstMirBuilder, hardware_metadata::HardwareMetadata,
    map_ir::MapIrBuilder,
};
use syl_hw::ParametricHwDesign;
use syl_sema::TirAnalysis;
use syl_span::Diagnostic;

#[non_exhaustive]
pub(super) struct TirStageRunner<'tir_stage> {
    tir: &'tir_stage TirAnalysis,
}

impl<'tir_stage> TirStageRunner<'tir_stage> {
    pub(super) fn new(tir: &'tir_stage TirAnalysis) -> Self {
        Self { tir }
    }

    pub(super) fn compile_hwir(&self) -> Result<ParametricHwDesign, CompileError> {
        let eir = self.elaborate_eir()?;
        eir.analyze_drivers()?;
        eir.lower_hwir()
    }

    pub(super) fn diagnostics(&self) -> Vec<Diagnostic> {
        self.stage_output().into_diagnostics()
    }

    pub(super) fn stage_output(&self) -> ElaborationOutput {
        ElaborationOutputBuilder::new(self.tir).run()
    }

    fn elaborate_eir(&self) -> Result<EirStage, CompileError> {
        let const_mir = ConstMirBuilder::new(self.tir.design())
            .build()
            .map(ConstMirStage::new)?;
        let map_ir = MapIrBuilder::new(self.tir.design())
            .build()
            .map(MapIrStage::new)?;
        ElabStage::from_tir(self.tir.design()).elaborate(&const_mir, &map_ir)
    }
}

#[non_exhaustive]
struct ElaborationOutputBuilder<'tir_stage> {
    tir: &'tir_stage TirAnalysis,
    const_mir: Option<ConstMirStage>,
    map_ir: Option<MapIrStage>,
    eir: Option<EirStage>,
    drivers: Option<DriverStage>,
    metadata: Option<HardwareMetadata>,
    hwir: Option<ParametricHwDesign>,
    diagnostics: Vec<Diagnostic>,
}

impl<'tir_stage> ElaborationOutputBuilder<'tir_stage> {
    fn new(tir: &'tir_stage TirAnalysis) -> Self {
        Self {
            tir,
            const_mir: None,
            map_ir: None,
            eir: None,
            drivers: None,
            metadata: None,
            hwir: None,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> ElaborationOutput {
        self.build_const_mir();
        self.build_map_ir();
        if self.const_mir.is_none() || self.map_ir.is_none() {
            return self.finish();
        }
        self.elaborate_eir();
        if self.eir.is_none() {
            return self.finish();
        }
        self.analyze_drivers();
        if self.drivers.is_none() {
            return self.finish();
        }
        self.lower_metadata();
        self.lower_hwir();
        self.finish()
    }

    fn build_const_mir(&mut self) {
        match ConstMirBuilder::new(self.tir.design()).build() {
            Ok(const_mir) => {
                self.const_mir = Some(ConstMirStage::new(const_mir));
            }
            Err(error) => {
                self.diagnostics.push(Diagnostic::from(error));
            }
        }
    }

    fn build_map_ir(&mut self) {
        match MapIrBuilder::new(self.tir.design()).build() {
            Ok(map_ir) => {
                self.map_ir = Some(MapIrStage::new(map_ir));
            }
            Err(error) => {
                self.diagnostics.push(Diagnostic::from(error));
            }
        }
    }

    fn elaborate_eir(&mut self) {
        let (Some(const_mir), Some(map_ir)) = (&self.const_mir, &self.map_ir) else {
            return;
        };
        let eir = match ElabStage::from_tir(self.tir.design()).elaborate(const_mir, map_ir) {
            Ok(eir) => eir,
            Err(error) => {
                self.diagnostics.push(Diagnostic::from(error));
                return;
            }
        };
        self.eir = Some(eir);
    }

    fn analyze_drivers(&mut self) {
        let Some(eir) = &self.eir else {
            return;
        };
        let drivers = match eir.analyze_drivers_collect() {
            Ok(facts) => facts,
            Err(errors) => {
                self.diagnostics
                    .extend(errors.into_iter().map(Diagnostic::from));
                return;
            }
        };
        self.drivers = Some(drivers);
    }

    fn lower_hwir(&mut self) {
        let Some(eir) = &self.eir else {
            return;
        };
        match eir.lower_hwir() {
            Ok(hwir) => {
                self.hwir = Some(hwir);
            }
            Err(error) => self.diagnostics.push(Diagnostic::from(error)),
        }
    }

    fn lower_metadata(&mut self) {
        let Some(drivers) = &self.drivers else {
            return;
        };
        match drivers.metadata() {
            Ok(metadata) => {
                self.metadata = Some(metadata);
            }
            Err(error) => self.diagnostics.push(Diagnostic::from(error)),
        }
    }

    fn finish(self) -> ElaborationOutput {
        ElaborationOutput {
            const_mir: self.const_mir,
            map_ir: self.map_ir,
            eir: self.eir,
            drivers: self.drivers,
            metadata: self.metadata,
            hwir: self.hwir,
            diagnostics: self.diagnostics,
        }
    }
}
