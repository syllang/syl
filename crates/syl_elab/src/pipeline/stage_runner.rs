use super::{
    ConstMirStage, DriverStage, EirStage, HirStage, MapIrStage, MiddleSession, TirStage,
    TirStageOutput,
};
use crate::CompileError;
use syl_hw::ParametricHwDesign;
use syl_span::Diagnostic;

#[non_exhaustive]
pub(super) struct StageRunner<'session, 'files> {
    session: &'session MiddleSession<'files>,
}

impl<'session, 'files> StageRunner<'session, 'files> {
    pub(super) fn new(session: &'session MiddleSession<'files>) -> Self {
        Self { session }
    }

    pub(super) fn compile_hwir(&self) -> Result<ParametricHwDesign, CompileError> {
        let hir = self.session.resolve_hir()?;
        HirStageRunner::new(&hir).compile_hwir()
    }

    pub(super) fn diagnostics(&self) -> Vec<Diagnostic> {
        let hir = match self.session.resolve_hir_collect() {
            Ok(hir) => hir,
            Err(errors) => return errors.into_iter().map(Diagnostic::from).collect(),
        };
        HirStageRunner::new(&hir).diagnostics()
    }
}

#[non_exhaustive]
pub(super) struct HirStageRunner<'hir_stage> {
    hir: &'hir_stage HirStage,
}

impl<'hir_stage> HirStageRunner<'hir_stage> {
    pub(super) fn new(hir: &'hir_stage HirStage) -> Self {
        Self { hir }
    }

    pub(super) fn compile_hwir(&self) -> Result<ParametricHwDesign, CompileError> {
        let tir = self.hir.check_tir()?;
        TirStageRunner::new(&tir).compile_hwir()
    }

    pub(super) fn diagnostics(&self) -> Vec<Diagnostic> {
        let tir = self.hir.check_tir_partial();
        if !tir.diagnostics().is_empty() {
            return tir.into_diagnostics();
        }
        let tir = tir
            .into_stage()
            .expect("partial TIR output should carry a stage when no diagnostics were emitted");
        TirStageRunner::new(&tir).diagnostics()
    }
}

#[non_exhaustive]
pub(super) struct TirStageRunner<'tir_stage> {
    tir: &'tir_stage TirStage,
}

impl<'tir_stage> TirStageRunner<'tir_stage> {
    pub(super) fn new(tir: &'tir_stage TirStage) -> Self {
        Self { tir }
    }

    fn compile_hwir(&self) -> Result<ParametricHwDesign, CompileError> {
        let eir = self.elaborate_eir()?;
        let facts = eir.analyze_drivers()?;
        facts.lower_hwir(&eir)
    }

    pub(super) fn diagnostics(&self) -> Vec<Diagnostic> {
        self.stage_output().into_diagnostics()
    }

    pub(super) fn stage_output(&self) -> TirStageOutput {
        TirStageOutputBuilder::new(self.tir).run()
    }

    fn elaborate_eir(&self) -> Result<EirStage, CompileError> {
        let const_mir = self.tir.build_const_mir()?;
        let map_ir = self.tir.build_map_ir()?;
        let elab = self.tir.build_program();
        elab.elaborate(&const_mir, &map_ir)
    }
}

#[non_exhaustive]
struct TirStageOutputBuilder<'tir_stage> {
    tir: &'tir_stage TirStage,
    const_mir: Option<ConstMirStage>,
    map_ir: Option<MapIrStage>,
    eir: Option<EirStage>,
    drivers: Option<DriverStage>,
    hwir: Option<ParametricHwDesign>,
    diagnostics: Vec<Diagnostic>,
}

impl<'tir_stage> TirStageOutputBuilder<'tir_stage> {
    fn new(tir: &'tir_stage TirStage) -> Self {
        Self {
            tir,
            const_mir: None,
            map_ir: None,
            eir: None,
            drivers: None,
            hwir: None,
            diagnostics: Vec::new(),
        }
    }

    fn run(mut self) -> TirStageOutput {
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
        self.lower_hwir();
        self.finish()
    }

    fn build_const_mir(&mut self) {
        match self.tir.build_const_mir() {
            Ok(const_mir) => {
                self.const_mir = Some(const_mir);
            }
            Err(error) => {
                self.diagnostics.push(Diagnostic::from(error));
            }
        }
    }

    fn build_map_ir(&mut self) {
        match self.tir.build_map_ir() {
            Ok(map_ir) => {
                self.map_ir = Some(map_ir);
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
        let elab = self.tir.build_program();
        let eir = match elab.elaborate(const_mir, map_ir) {
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
        let (Some(eir), Some(drivers)) = (&self.eir, &self.drivers) else {
            return;
        };
        match drivers.lower_hwir(eir) {
            Ok(hwir) => {
                self.hwir = Some(hwir);
            }
            Err(error) => self.diagnostics.push(Diagnostic::from(error)),
        }
    }

    fn finish(self) -> TirStageOutput {
        TirStageOutput {
            const_mir: self.const_mir,
            map_ir: self.map_ir,
            eir: self.eir,
            drivers: self.drivers,
            hwir: self.hwir,
            diagnostics: self.diagnostics,
        }
    }
}
