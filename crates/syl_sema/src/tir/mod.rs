mod binding;
mod builtin;
mod check;
mod checker;
mod consts;
mod design;
mod enum_layout;
mod extension_method;
mod phase;
mod return_type;
#[cfg(test)]
mod type_identity_tests;
mod type_system;

pub use binding::{BindingKind, BindingRef};
pub use builtin::{BuiltinIntrinsic, BuiltinResolver};
pub(crate) use checker::HardwareBlockMode;
pub use checker::TypePhaseChecker;
pub use design::TirDesign;
pub use phase::Phase;
pub use type_system::{TirConstTerm, TirGenericArg, TirType, TirTypeTable, TypeId};
