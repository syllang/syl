pub use syl_sema::CompileError;
#[cfg(test)]
pub(crate) use syl_sema::LoweringError;
pub(crate) use syl_sema::{ConstEvalError, DriverError, EirError, TirError};

mod actual_binding;
mod const_eval;
mod const_mir;
mod driver;
mod driver_place;
mod eir;
mod eir_cell;
mod eir_builder;
mod hardware_metadata;
mod hardware_metadata_lower;
mod hw_lower;
mod map_ir;
mod mir;
mod pipeline;
mod program;
mod source;
mod tir;

pub use hardware_metadata::{
    HardwareCellSummary, HardwareCellSummaryBuilder, HardwareCreateFact, HardwareCreateKind,
    HardwareDriveFact, HardwareMetadata, HardwareReadFact,
};
pub use pipeline::{
    ConstMirStage, DrcStage, DriverFactsStage, EirBuildStage, EirFactsStage, EirStage,
    EirValidationStage, ElabStage, ElaborationOutput, HardwareCompiler, MapIrStage,
};
pub(crate) use syl_sema::cell_summary::CellBoundarySummary;
