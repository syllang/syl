mod ids;
pub use ids::TypeId;

pub mod actual_binding;
mod capability;
mod capability_model;
pub mod cell_summary;
pub mod completion;
pub mod const_eval;
pub mod const_mir;
pub mod diagnostic;
pub mod error;
mod hir;
mod hir_lower;
mod hir_resolve;
mod hir_view;
pub mod map_ir;
pub mod mir;
mod mir_type_resolve;
mod stage_output;
pub mod tir;
mod tir_const;

pub use diagnostic::{SemanticDiagnostic, SemanticDiagnosticStage};
pub use error::{
    CapabilityError, CompileError, ConstEvalError, DriverError, EirError, HirError, HwirError,
    LoweringError, TirError,
};
pub use hir_lower::HirResolver;
pub use stage_output::StageOutput;
