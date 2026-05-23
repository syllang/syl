pub(crate) use syl_sema::CompileError;
#[cfg(test)]
pub(crate) use syl_sema::LoweringError;
pub(crate) use syl_sema::{ConstEvalError, DriverError, EirError, TirError};

mod actual_binding;
mod completion;
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
mod hw_lower;
mod map_ir;
mod mir;
mod pipeline;
mod program;
mod source;
mod tir;

pub use pipeline::{
    ConstMirStage, DefinitionInfo, DriverStage, EirStage, ElabStage, HirStage, HirStageOutput,
    HoverInfo, MapIrStage, MiddleCompiler, MiddleOutput, MiddleSession, TirStage, TirStageOutput,
};
pub(crate) use syl_sema::cell_summary::CellBoundarySummary;
