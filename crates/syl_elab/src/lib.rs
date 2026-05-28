pub use syl_sema::CompileError;
#[cfg(test)]
pub(crate) use syl_sema::LoweringError;
pub(crate) use syl_sema::{ConstEvalError, DriverError, EirError, TirError};

mod const_eval;
mod const_mir;
mod driver;
mod eir;
mod hw;
mod map_ir;
mod metadata;
mod mir;
mod pipeline;
mod program;
mod source;
mod tir;

pub use metadata::{
    HardwareCellSummary, HardwareCellSummaryBuilder, HardwareCreateFact, HardwareCreateKind,
    HardwareDriveFact, HardwareMetadata, HardwareReadFact,
};
pub use pipeline::{
    ConstMirStage, DrcStage, DriverFactsStage, EirBuildStage, EirFactsStage, EirStage,
    EirValidationStage, ElabStage, ElaborationOutput, HardwareCompiler, MapIrStage,
};
