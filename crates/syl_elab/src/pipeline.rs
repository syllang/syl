use crate::{
    CompileError,
    const_mir::ConstMirProgram,
    driver::{DriverDrcReport, DriverFacts},
    eir::{EirDesign, EirDesignFacts, EirItem, EirModule, EirRawDesign},
    hardware_metadata::HardwareMetadata,
    map_ir::MapIrProgram,
};
use std::{fmt, sync::Arc};
use syl_hw::ParametricHwDesign;
use syl_sema::TirAnalysis;
use syl_span::Diagnostic;

mod debug;
mod stage;
mod stage_runner;

pub use stage::ElabStage;
use stage_runner::TirStageRunner;

#[derive(Debug, Default)]
#[non_exhaustive]
pub struct HardwareCompiler;

impl HardwareCompiler {
    pub fn new() -> Self {
        Self
    }

    pub fn compile_tir(&self, tir: &TirAnalysis) -> Result<ParametricHwDesign, CompileError> {
        TirStageRunner::new(tir).compile_hwir()
    }

    pub fn output_for_tir(&self, tir: &TirAnalysis) -> ElaborationOutput {
        TirStageRunner::new(tir).stage_output()
    }

    pub fn diagnostics(&self, tir: &TirAnalysis) -> Vec<Diagnostic> {
        TirStageRunner::new(tir).diagnostics()
    }
}

#[non_exhaustive]
pub struct ElaborationOutput {
    const_mir: Option<ConstMirStage>,
    map_ir: Option<MapIrStage>,
    eir_build: Option<EirBuildStage>,
    eir_validation: Option<EirValidationStage>,
    eir_facts: Option<EirFactsStage>,
    eir: Option<EirStage>,
    driver_facts: Option<DriverFactsStage>,
    drc: Option<DrcStage>,
    metadata: Option<HardwareMetadata>,
    hwir: Option<ParametricHwDesign>,
    diagnostics: Vec<Diagnostic>,
}

impl ElaborationOutput {
    pub fn const_mir(&self) -> Option<&ConstMirStage> {
        self.const_mir.as_ref()
    }

    pub fn map_ir(&self) -> Option<&MapIrStage> {
        self.map_ir.as_ref()
    }

    pub fn eir_build(&self) -> Option<&EirBuildStage> {
        self.eir_build.as_ref()
    }

    pub fn eir_validation(&self) -> Option<&EirValidationStage> {
        self.eir_validation.as_ref()
    }

    pub fn eir_facts(&self) -> Option<&EirFactsStage> {
        self.eir_facts.as_ref()
    }

    pub fn eir(&self) -> Option<&EirStage> {
        self.eir.as_ref()
    }

    pub fn driver_facts(&self) -> Option<&DriverFactsStage> {
        self.driver_facts.as_ref()
    }

    pub fn drc(&self) -> Option<&DrcStage> {
        self.drc.as_ref()
    }

    pub fn metadata(&self) -> Option<&HardwareMetadata> {
        self.metadata.as_ref()
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

    pub fn debug_dump(&self) -> String {
        self.program.debug_dump()
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

    pub fn debug_dump(&self) -> String {
        self.program.debug_dump()
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
pub struct EirBuildStage {
    design: Arc<EirRawDesign>,
}

impl EirBuildStage {
    fn new(design: EirRawDesign) -> Self {
        Self {
            design: Arc::new(design),
        }
    }

    pub fn debug_dump(&self) -> String {
        debug::eir_build_stage_dump(self)
    }

    pub fn module_count(&self) -> usize {
        self.design.modules().len()
    }

    pub fn contains_cell_expansion(&self, callable: &str, instance: &str) -> bool {
        self.design.modules().iter().any(|module| {
            Self::module_contains_item(module, |item| {
                matches!(
                    item,
                    EirItem::CellExpansion(expansion)
                        if expansion.callable() == callable && expansion.instance() == instance
                )
            })
        })
    }

    pub fn contains_instance(&self, module_name: &str, instance_name: &str) -> bool {
        self.design.modules().iter().any(|module| {
            Self::module_contains_item(module, |item| {
                matches!(
                    item,
                    EirItem::Instance(instance)
                        if instance.module() == module_name && instance.name() == instance_name
                )
            })
        })
    }

    pub fn contains_instance_module(&self, module_name: &str) -> bool {
        self.design.modules().iter().any(|module| {
            Self::module_contains_item(
                module,
                |item| matches!(item, EirItem::Instance(instance) if instance.module() == module_name),
            )
        })
    }

    fn module_contains_item(
        module: &EirModule,
        mut predicate: impl FnMut(&EirItem) -> bool,
    ) -> bool {
        let mut stack = vec![module.items()];
        while let Some(items) = stack.pop() {
            for item in items {
                if predicate(item) {
                    return true;
                }
                match item {
                    EirItem::CellExpansion(expansion) => stack.push(expansion.items()),
                    EirItem::SymbolicStaticIf {
                        then_items,
                        else_items,
                        ..
                    } => {
                        stack.push(then_items);
                        stack.push(else_items);
                    }
                    EirItem::SymbolicStaticFor { items, .. } => stack.push(items),
                    EirItem::StaticParam { .. }
                    | EirItem::Signal { .. }
                    | EirItem::Storage { .. }
                    | EirItem::Drive { .. }
                    | EirItem::ClockedStorage { .. }
                    | EirItem::CellBoundary(_)
                    | EirItem::Instance(_)
                    | EirItem::InitialError { .. } => {}
                }
            }
        }
        false
    }
}

#[non_exhaustive]
pub struct EirValidationStage {
    module_count: usize,
}

impl EirValidationStage {
    fn new(module_count: usize) -> Self {
        Self { module_count }
    }

    pub fn debug_dump(&self) -> String {
        debug::eir_validation_stage_dump(self)
    }

    pub fn module_count(&self) -> usize {
        self.module_count
    }
}

#[non_exhaustive]
pub struct EirFactsStage {
    facts: Arc<EirDesignFacts>,
}

impl EirFactsStage {
    fn new(facts: EirDesignFacts) -> Self {
        Self {
            facts: Arc::new(facts),
        }
    }

    pub fn debug_dump(&self) -> String {
        debug::eir_facts_stage_dump(self)
    }

    pub fn object_count(&self) -> usize {
        self.facts.objects().len()
    }

    pub fn drive_count(&self) -> usize {
        self.facts.drives().len()
    }

    pub fn read_count(&self) -> usize {
        self.facts.reads().len()
    }

    pub fn contains_created_object(&self, module: &str, name: &str) -> bool {
        self.facts
            .objects()
            .iter()
            .any(|object| object.module() == module && object.name() == name)
    }

    pub fn contains_drive(&self, module: &str, target: &str) -> bool {
        self.facts.drives().iter().any(|drive| {
            drive.module() == module && drive.target_place().to_expr().fact_key() == target
        })
    }

    pub fn contains_read(&self, module: &str, source: &str) -> bool {
        self.facts.reads().iter().any(|read| {
            read.module() == module && read.source_place().to_expr().fact_key() == source
        })
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

    pub fn debug_dump(&self) -> String {
        debug::eir_stage_dump(self)
    }

    pub fn module_count(&self) -> usize {
        self.design.modules().len()
    }

    pub fn drive_count(&self) -> usize {
        self.design.drives().len()
    }
}

#[non_exhaustive]
pub struct DriverFactsStage {
    facts: DriverFacts,
}

impl DriverFactsStage {
    fn new(facts: DriverFacts) -> Self {
        Self { facts }
    }

    pub fn debug_dump(&self) -> String {
        debug::driver_facts_stage_dump(self)
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
}

#[non_exhaustive]
pub struct DrcStage {
    report: DriverDrcReport,
}

impl DrcStage {
    fn new(report: DriverDrcReport) -> Self {
        Self { report }
    }

    pub fn debug_dump(&self) -> String {
        debug::drc_stage_dump(self)
    }

    pub fn module_count(&self) -> usize {
        self.report.module_count()
    }

    pub fn drive_count(&self) -> usize {
        self.report.drive_count()
    }

    pub fn read_count(&self) -> usize {
        self.report.read_count()
    }

    pub fn create_count(&self) -> usize {
        self.report.create_count()
    }
}

impl fmt::Debug for ElaborationOutput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ElaborationOutput")
            .field("has_const_mir", &self.const_mir.is_some())
            .field("has_map_ir", &self.map_ir.is_some())
            .field("has_eir_build", &self.eir_build.is_some())
            .field("has_eir_validation", &self.eir_validation.is_some())
            .field("has_eir_facts", &self.eir_facts.is_some())
            .field("has_eir", &self.eir.is_some())
            .field("has_driver_facts", &self.driver_facts.is_some())
            .field("has_drc", &self.drc.is_some())
            .field("has_metadata", &self.metadata.is_some())
            .field("has_hwir", &self.hwir.is_some())
            .field("diagnostic_count", &self.diagnostics.len())
            .finish()
    }
}

impl fmt::Debug for ConstMirStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConstMirStage")
            .field("node_count", &self.node_count())
            .field("local_ref_count", &self.local_ref_count())
            .field("resolved_local_ref_count", &self.resolved_local_ref_count())
            .finish()
    }
}

impl fmt::Debug for MapIrStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("MapIrStage")
            .field("map_count", &self.map_count())
            .field("param_count", &self.param_count())
            .field("resolved_param_count", &self.resolved_param_count())
            .field("local_ref_count", &self.local_ref_count())
            .field("resolved_local_ref_count", &self.resolved_local_ref_count())
            .finish()
    }
}

impl fmt::Debug for EirBuildStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EirBuildStage")
            .field("module_count", &self.module_count())
            .finish()
    }
}

impl fmt::Debug for EirValidationStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EirValidationStage")
            .field("module_count", &self.module_count())
            .finish()
    }
}

impl fmt::Debug for EirFactsStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EirFactsStage")
            .field("object_count", &self.object_count())
            .field("drive_count", &self.drive_count())
            .field("read_count", &self.read_count())
            .finish()
    }
}

impl fmt::Debug for EirStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EirStage")
            .field("module_count", &self.module_count())
            .field("drive_count", &self.drive_count())
            .finish()
    }
}

impl fmt::Debug for DriverFactsStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DriverFactsStage")
            .field("drive_count", &self.drive_count())
            .field("read_count", &self.read_count())
            .field("create_count", &self.create_count())
            .finish()
    }
}

impl fmt::Debug for DrcStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DrcStage")
            .field("module_count", &self.module_count())
            .field("drive_count", &self.drive_count())
            .field("read_count", &self.read_count())
            .field("create_count", &self.create_count())
            .finish()
    }
}
