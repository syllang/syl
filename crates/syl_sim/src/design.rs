use std::collections::BTreeMap;
use syl_elab::ElaborationOutput;
use syl_hw::{
    HwBinaryOp, HwDirection, HwOrigin, HwResetMode, HwResetPolarity, HwSelectMode, HwUnaryOp,
};
use syl_span::Span;

macro_rules! ids {
    ($($name:ident),+) => {$(
        #[doc = "Dense index in the corresponding arena of this concrete design."]
        #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(pub usize);
    )+}
}
ids!(
    InstanceId, SignalId, RegId, MemId, AssertId, DomainId, ResetId, TemplateId, CombId
);

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum ParamValue {
    Nat(u64),
    Bool(bool),
}

#[derive(Clone, Debug)]
pub struct SourceLocation {
    pub instance: InstanceId,
    pub path: String,
    pub name: String,
    pub origin: HwOrigin,
}

impl SourceLocation {
    pub fn span(&self) -> Span {
        Span::new_in(
            self.origin.source(),
            self.origin.span_start(),
            self.origin.span_end(),
        )
    }
}

#[derive(Clone, Debug)]
pub struct SimSignal {
    /// Parent net of a connected inout port. Reads and drives use this target.
    pub alias: Option<SimTarget>,
    pub source: SourceLocation,
    pub width: u32,
}

#[derive(Clone, Debug)]
pub struct SimPort {
    pub signal: SignalId,
    pub direction: HwDirection,
    /// The physical port name, including interface flattening.
    pub name: String,
}

#[derive(Clone, Debug)]
pub struct SimInstance {
    pub source: SourceLocation,
    pub parent: Option<InstanceId>,
    pub template: TemplateId,
    pub ports: Vec<SimPort>,
}

/// Instances of one canonical module with identical evaluated parameter bindings.
#[derive(Clone, Debug)]
pub struct InstanceFamily {
    pub module: Vec<String>,
    pub parameters: BTreeMap<String, ParamValue>,
    pub members: Vec<InstanceId>,
}

#[derive(Clone, Debug)]
pub struct ClockDomain {
    pub signal: SignalId,
    pub name: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ResetDomain {
    pub signal: SignalId,
    pub name: Option<String>,
    pub mode: HwResetMode,
    pub polarity: HwResetPolarity,
}

/// A part index counts words of the given width (a bit index has width one).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Selector {
    Slice { high: u32, low: u32 },
    Index { index: Box<SimExpr>, width: u32 },
}

impl Selector {
    pub(crate) fn width(&self) -> u32 {
        match self {
            Self::Slice { high, low } => high - low + 1,
            Self::Index { width, .. } => *width,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SimTarget {
    pub signal: SignalId,
    pub selectors: Vec<Selector>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SimExpr {
    Read(SimTarget),
    Int(u64),
    Bool(bool),
    HighZ,
    Zero,
    Unary {
        op: HwUnaryOp,
        expr: Box<SimExpr>,
    },
    Binary {
        op: HwBinaryOp,
        left: Box<SimExpr>,
        right: Box<SimExpr>,
    },
    Mux {
        cond: Box<SimExpr>,
        then_value: Box<SimExpr>,
        else_value: Box<SimExpr>,
    },
    Select {
        mode: HwSelectMode,
        arms: Vec<(SimExpr, SimExpr)>,
        default: Box<SimExpr>,
    },
    Concat(Vec<SimExpr>),
    Project {
        value: Box<SimExpr>,
        selector: Selector,
    },
    MemoryRead {
        memory: MemId,
        port: usize,
    },
}

#[derive(Clone, Debug)]
pub struct CombAssign {
    pub span: Span,
    pub target: SimTarget,
    pub value: SimExpr,
}

#[derive(Clone, Debug)]
pub struct SimReg {
    pub source: SourceLocation,
    pub signal: SignalId,
    pub width: u32,
    pub clock: DomainId,
    pub reset: Option<(ResetId, SimExpr)>,
    pub next: SimExpr,
    pub next_span: Span,
}

#[derive(Clone, Debug)]
pub struct MemWritePort {
    pub clock: DomainId,
    pub enable: SimExpr,
    pub address: SimExpr,
    pub data: SimExpr,
}
#[derive(Clone, Debug)]
pub struct MemSyncReadPort {
    pub clock: DomainId,
    pub enable: SimExpr,
    pub address: SimExpr,
    pub target: SimTarget,
}
#[derive(Clone, Debug)]
pub struct MemCombReadPort {
    pub address: SimExpr,
}

#[derive(Clone, Debug)]
pub struct SimMem {
    pub source: SourceLocation,
    pub width: u32,
    pub depth: u32,
    pub writes: Vec<MemWritePort>,
    pub sync_reads: Vec<MemSyncReadPort>,
    pub comb_reads: Vec<MemCombReadPort>,
}

#[derive(Clone, Debug)]
pub struct SimAssert {
    pub source: SourceLocation,
    pub clock: DomainId,
    pub reset_mask: Option<ResetId>,
    pub condition: SimExpr,
    pub message: String,
}

#[derive(Clone, Debug)]
pub struct SimExternInstance {
    /// Formal clock ports whose transitions sample model inputs as a group.
    pub clocks: Vec<SignalId>,
    pub instance: InstanceId,
    /// Concrete external formal signals and their parent-side expressions.
    pub connections: Vec<(SignalId, SimExpr)>,
}

#[derive(Debug, thiserror::Error)]
pub enum SimError {
    #[error("elaboration failed: {0}")]
    Elaboration(String),
    #[error("invalid simulation design: {0}")]
    Invalid(String),
    #[error("combinational cycle through {chain:?}")]
    CombinationalCycle {
        /// Closed dependency walk in signal path order.
        chain: Vec<String>,
        span: Span,
        /// Every assignment target in the strongly connected component.
        members: Vec<String>,
    },
}

#[derive(Default, Debug)]
pub struct SimDesign {
    /// Instance zero is the top. IDs index these vectors directly.
    pub instances: Vec<SimInstance>,
    pub families: Vec<InstanceFamily>,
    pub signals: Vec<SimSignal>,
    pub regs: Vec<SimReg>,
    pub memories: Vec<SimMem>,
    pub assertions: Vec<SimAssert>,
    pub clocks: Vec<ClockDomain>,
    pub resets: Vec<ResetDomain>,
    pub externs: Vec<SimExternInstance>,
    pub comb: Vec<CombAssign>,
    pub comb_eval_order: Vec<CombId>,
    /// Extern evaluations inserted before the given index in comb_eval_order.
    pub extern_slots: Vec<(usize, InstanceId)>,
}

impl SimDesign {
    pub fn lower(
        output: &ElaborationOutput,
        top: &str,
        parameters: &BTreeMap<String, ParamValue>,
    ) -> Result<Self, SimError> {
        let hw = output
            .hwir()
            .ok_or_else(|| SimError::Elaboration(format!("{:?}", output.diagnostics())))?;
        crate::lower::concretize(hw, top, parameters)
    }

    pub fn from_hwir(
        hw: &syl_hw::ParametricHwDesign,
        top: &str,
        parameters: &BTreeMap<String, ParamValue>,
    ) -> Result<Self, SimError> {
        crate::lower::concretize(hw, top, parameters)
    }

    pub fn signal(&self, id: SignalId) -> &SimSignal {
        &self.signals[id.0]
    }
    pub fn register(&self, id: RegId) -> &SimReg {
        &self.regs[id.0]
    }
    pub fn assertion(&self, id: AssertId) -> &SimAssert {
        &self.assertions[id.0]
    }
    pub fn top_ports(&self) -> &[SimPort] {
        self.instances
            .first()
            .map_or(&[], |instance| instance.ports.as_slice())
    }
}
