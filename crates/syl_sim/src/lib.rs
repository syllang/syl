#![doc = include_str!("../README.md")]

mod constant;
mod design;
pub mod gpu_cuda;
mod lower;
pub mod runtime;
mod schedule;
pub mod value;
pub mod vcd;

pub use design::{
    AssertId, ClockDomain, CombAssign, CombId, DomainId, InstanceFamily, InstanceId,
    MemCombReadPort, MemId, MemSyncReadPort, MemWritePort, ParamValue, RegId, ResetDomain, ResetId,
    Selector, SignalId, SimAssert, SimDesign, SimError, SimExpr, SimExternInstance, SimInstance,
    SimMem, SimPort, SimReg, SimSignal, SimTarget, SourceLocation, TemplateId,
};
pub use syl_hw::{HwBinaryOp, HwDirection, HwResetMode, HwResetPolarity, HwSelectMode, HwUnaryOp};

pub use runtime::{
    AssertionFailure, ClockEvent, ExternModel, RuntimeError, Simulator, StateMode,
    UniqueOverlapViolation,
};
pub use value::{Logic, Value};
pub use vcd::VcdWriter;
