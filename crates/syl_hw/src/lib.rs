mod ids;
pub use ids::ObjectId;

mod format;
pub use format::{FormatPiece, FormatStyle};
mod design;
mod reset;
pub use reset::{HwPortDomain, HwResetMode, HwResetPolarity};
mod expr;
mod parametric;
mod place;
mod validate;

pub use design::{
    HwConnection, HwDesign, HwDirection, HwExpansion, HwGuard, HwGuardFrame, HwInstance, HwItem,
    HwModule, HwOrigin, HwParam, HwParamBind, HwPort, HwReset,
};
pub use expr::{HwBinaryOp, HwExpr, HwSelectArm, HwSelectMode, HwUnaryOp};
pub use parametric::{ParametricHwDesign, ParametricHwItem, ParametricHwModule};
pub use place::{HwPlace, HwPlaceExpr};
pub use validate::{
    HwBindingKind, HwNormalizer, HwValidationDiagnostic, HwValidationReport, HwValidator,
    NormalizedParametricHwDesign,
};
