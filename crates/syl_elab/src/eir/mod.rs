//! EIR (Elaboration Intermediate Representation) data model.
//!
//! Split by responsibility:
//! - `core` - shared EIR syntax and origin/guard types
//! - `design` - top-level containers
//! - `signal` - signal objects and their behavior
//! - `module` - module structure, ports, parameters, instantiation

mod core;
mod design;
mod module;
mod signal;

mod assemble;
mod facts;
mod validate;

pub(crate) use core::{
    EirBinaryOp, EirBound, EirExpr, EirExpansion, EirGuard, EirGuardFrame, EirGuardLabel,
    EirOrigin, EirPlace, EirSelectArm, EirSelectMode, EirUnaryOp,
};
pub(crate) use design::{EirDesign, EirDesignFacts, EirRawDesign};
pub(crate) use module::{
    EirConnection, EirDirection, EirInstance, EirItem, EirModule, EirParam, EirParamBind, EirPort,
};
pub(crate) use signal::{
    EirDrive, EirDriveInput, EirDriveKind, EirObject, EirObjectInput, EirObjectKind, EirRead,
    EirReset, EirSignalActivity,
};

pub(crate) use assemble::EirDesignComposer;
pub(crate) use facts::EirFactCollector;
pub(crate) use validate::EirValidator;
