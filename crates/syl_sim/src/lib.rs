#![doc = include_str!("../README.md")]

mod batch_cpu;
mod constant;
mod design;
mod fast_cpu;
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
    SimLog, SimMem, SimPort, SimReg, SimSignal, SimTarget, SourceLocation, TemplateId,
};
pub use syl_hw::{HwBinaryOp, HwDirection, HwResetMode, HwResetPolarity, HwSelectMode, HwUnaryOp};

pub use batch_cpu::{BatchCpuSimulator, BatchFailure};
pub use fast_cpu::{FastBatchCpuSimulator, FastProgram, FastTile};
pub use runtime::{
    AssertionFailure, ClockEvent, ExternModel, RuntimeError, Simulator, StateMode,
    UniqueOverlapViolation,
};
pub use value::{Logic, Value};
pub use vcd::VcdWriter;

mod backend;
mod batch_run;
pub use backend::{
    BackendRequest, BackendSelection, GpuKernelMode, LocalBackend, ResolvedBackend,
    SimBackendChoice, select_backend, select_local_backend,
};
pub use batch_run::{
    BatchOptions, BatchResult, FailureCoordinate, batch_stimulus_word, replay_failure, run_batch,
};
