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
mod eir_body;
mod eir_build;
mod eir_cell;
mod eir_connect;
mod eir_const;
mod eir_expr;
mod eir_guard;
mod eir_map;
mod eir_origin;
mod eir_place;
mod eir_type;
mod eir_value;
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
    ConstMirStage, DriverStage, EirStage, ElabStage, ElaborationOutput, HardwareCompiler,
    MapIrStage,
};
pub(crate) use syl_sema::cell_summary::CellBoundarySummary;
