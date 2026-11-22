#![doc = include_str!("../README.md")]

mod constant;
mod design;
mod lower;
mod schedule;

pub use design::{
    AssertId, ClockDomain, CombAssign, CombId, DomainId, InstanceFamily, InstanceId,
    MemCombReadPort, MemId, MemSyncReadPort, MemWritePort, ParamValue, RegId, ResetDomain, ResetId,
    Selector, SignalId, SimAssert, SimDesign, SimError, SimExpr, SimExternInstance, SimInstance,
    SimMem, SimPort, SimReg, SimSignal, SimTarget, SourceLocation, TemplateId,
};
pub use syl_hw::{HwBinaryOp, HwDirection, HwResetMode, HwResetPolarity, HwSelectMode, HwUnaryOp};
