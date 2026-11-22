//! Event-driven reference execution of a concrete design.
mod expr;
use crate::*;
use std::collections::{BTreeMap, BTreeSet};
use syl_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClockEvent {
    Rising(DomainId),
    Falling(DomainId),
}
impl ClockEvent {
    fn domain(self) -> DomainId {
        match self {
            Self::Rising(id) | Self::Falling(id) => id,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cycle {cycle}, {path}: {message}")]
pub struct AssertionFailure {
    pub cycle: u64,
    pub path: String,
    pub span: Span,
    pub message: String,
}
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[error("cycle {cycle}, {path}: overlapping guards in select unique")]
pub struct UniqueOverlapViolation {
    pub cycle: u64,
    pub path: String,
    pub span: Span,
}
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Assertion(#[from] AssertionFailure),
    #[error(transparent)]
    UniqueOverlap(#[from] UniqueOverlapViolation),
    #[error("{0}")]
    Invalid(String),
}
/// Evaluated in dependency order after the instance inputs settle. Each output
/// is required. Models may retain state between input changes.
pub trait ExternModel {
    fn eval(
        &mut self,
        inputs: &BTreeMap<String, Value>,
    ) -> std::result::Result<BTreeMap<String, Value>, String>;
}
struct External {
    model: Box<dyn ExternModel>,
    previous: Option<BTreeMap<String, Value>>,
    outputs: Vec<(SignalId, Value)>,
}

/// Four-state execution retains uninitialized memory and register bits. The
/// optional two-state projection initializes storage to zero and projects
/// unknown expression results to zero, including out-of-range reads.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Default)]
pub enum StateMode {
    #[default]
    FourState,
    TwoState,
}

pub struct Simulator<'d> {
    pub design: &'d SimDesign,
    mode: StateMode,
    clock_sources: Vec<SignalId>,
    values: Vec<Value>,
    memories: Vec<Vec<Value>>,
    inout_drives: BTreeMap<SignalId, Value>,
    models: BTreeMap<InstanceId, External>,
    cycle: u64,
    pub strict_unique: bool,
    pub unique_violations: Vec<UniqueOverlapViolation>,
}
type Result<T> = std::result::Result<T, RuntimeError>;
#[derive(Clone, Copy)]
struct Site<'a> {
    path: &'a str,
    span: Span,
}
impl<'a> From<&'a SourceLocation> for Site<'a> {
    fn from(s: &'a SourceLocation) -> Self {
        Self {
            path: &s.path,
            span: s.span(),
        }
    }
}

impl<'d> Simulator<'d> {
    pub fn new(design: &'d SimDesign) -> Self {
        Self::with_mode(design, StateMode::FourState)
    }
    pub fn with_mode(design: &'d SimDesign, mode: StateMode) -> Self {
        let initial = if mode == StateMode::FourState {
            Value::x
        } else {
            Value::zero
        };

        // A local clock can be a whole-net copy of a top clock (or Bit).
        // Keep its IR identity for diagnostics while sharing physical edges.
        let copies: BTreeMap<_, _> = design
            .comb
            .iter()
            .filter_map(|a| {
                if let SimExpr::Read(t) = &a.value
                    && t.selectors.is_empty()
                    && a.target.selectors.is_empty()
                {
                    Some((a.target.signal, t.signal))
                } else {
                    None
                }
            })
            .collect();
        let clock_sources = design
            .clocks
            .iter()
            .map(|c| {
                let mut signal = c.signal;
                for _ in 0..copies.len() {
                    let Some(&parent) = copies.get(&signal) else {
                        break;
                    };
                    signal = parent;
                }
                signal
            })
            .collect();
        Self {
            design,
            mode,
            clock_sources,
            values: design.signals.iter().map(|s| initial(s.width)).collect(),
            memories: design
                .memories
                .iter()
                .map(|m| vec![initial(m.width); m.depth as usize])
                .collect(),
            inout_drives: design
                .instances
                .iter()
                .flat_map(|i| &i.ports)
                .filter(|p| p.direction == HwDirection::InOut)
                .map(|p| {
                    let signal = design
                        .signal(p.signal)
                        .alias
                        .as_ref()
                        .map_or(p.signal, |t| t.signal);
                    (signal, Value::z(design.signal(signal).width))
                })
                .collect(),
            models: BTreeMap::new(),
            cycle: 0,
            strict_unique: false,
            unique_violations: Vec::new(),
        }
    }
    fn project(&self, v: Value) -> Value {
        if self.mode == StateMode::TwoState {
            v.into_two_state()
        } else {
            v
        }
    }
    /// Number of committed groups containing at least one rising edge.
    pub fn cycle(&self) -> u64 {
        self.cycle
    }
    pub fn port(&self, name: &str) -> Result<SignalId> {
        self.design
            .top_ports()
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.signal)
            .ok_or_else(|| RuntimeError::Invalid(format!("unknown top port {name}")))
    }
    pub fn clock(&self, name: &str) -> Result<DomainId> {
        let signal = self.port(name)?;
        self.clock_sources
            .iter()
            .position(|s| *s == signal)
            .map(DomainId)
            .ok_or_else(|| RuntimeError::Invalid(format!("{name} is not a clock")))
    }
    /// Drive one input. Call eval_comb after a batch of simultaneous drives.
    pub fn drive(&mut self, id: SignalId, value: Value) -> Result<()> {
        if !self
            .design
            .top_ports()
            .iter()
            .any(|p| p.signal == id && p.direction != HwDirection::Out)
        {
            return Err(RuntimeError::Invalid(format!("{id:?} is not a top input")));
        }
        let v = self.project(value.resized(self.design.signal(id).width));
        if let Some(driver) = self.inout_drives.get_mut(&id) {
            driver.clone_from(&v);
        }
        self.values[id.0] = v;
        Ok(())
    }
    pub fn drive_named(&mut self, name: &str, value: Value) -> Result<()> {
        self.drive(self.port(name)?, value)
    }
    pub fn read(&self, id: SignalId) -> &Value {
        &self.values[id.0]
    }
    pub fn read_named(&self, name: &str) -> Result<&Value> {
        Ok(self.read(self.port(name)?))
    }
    pub fn memory(&self, id: MemId, address: u32) -> Option<&Value> {
        self.memories.get(id.0)?.get(address as usize)
    }
    pub fn register_extern(
        &mut self,
        id: InstanceId,
        model: impl ExternModel + 'static,
    ) -> Result<()> {
        if !self.design.externs.iter().any(|e| e.instance == id) {
            return Err(RuntimeError::Invalid(format!(
                "{id:?} is not an extern instance"
            )));
        }
        self.models.insert(
            id,
            External {
                model: Box::new(model),
                previous: None,
                outputs: Vec::new(),
            },
        );
        Ok(())
    }
    fn external(&mut self, id: InstanceId) -> Result<()> {
        let inst = &self.design.instances[id.0];
        let path = &inst.source.path;
        let mut inputs = BTreeMap::new();
        for port in inst
            .ports
            .iter()
            .filter(|p| p.direction != HwDirection::Out)
        {
            let signal = self.design.signal(port.signal);
            let v = if let Some(t) = &signal.alias {
                self.read_target(t, (&inst.source).into())?
            } else {
                self.read(port.signal).clone()
            };
            inputs.insert(port.name.clone(), v);
        }
        let entry = self
            .models
            .get_mut(&id)
            .ok_or_else(|| RuntimeError::Invalid(format!("unregistered extern cell {path}")))?;
        if entry.previous.as_ref() != Some(&inputs) {
            let mut outputs = entry
                .model
                .eval(&inputs)
                .map_err(|m| RuntimeError::Invalid(format!("{path}: {m}")))?;
            let mut drives = Vec::new();
            for p in inst.ports.iter().filter(|p| p.direction != HwDirection::In) {
                let v = outputs.remove(&p.name).ok_or_else(|| {
                    RuntimeError::Invalid(format!("missing extern output {path}.{}", p.name))
                })?;
                drives.push((p.signal, v.resized(self.design.signal(p.signal).width)));
            }
            if let Some(n) = outputs.keys().next() {
                return Err(RuntimeError::Invalid(format!(
                    "unknown extern output {path}.{n}"
                )));
            }
            entry.outputs = drives;
            entry.previous = Some(inputs);
        }
        for (net, v) in entry.outputs.clone() {
            let v = self.project(v);
            let target = if let Some(t) = &self.design.signal(net).alias {
                self.target(t, (&inst.source).into())?
            } else {
                Some((net, 0))
            };
            if let Some((id, low)) = target {
                self.assign(id, low, &v);
            }
        }
        Ok(())
    }
    fn assign(&mut self, id: SignalId, low: u32, v: &Value) {
        if self.inout_drives.contains_key(&id) {
            self.values[id.0].resolve(low, v);
            if self.mode == StateMode::TwoState {
                self.values[id.0] = self.values[id.0].clone().into_two_state();
            }
        } else {
            self.values[id.0].insert(low, v)
        }
    }
    fn propagate(&mut self) -> Result<()> {
        for (id, v) in &self.inout_drives {
            self.values[id.0] = self.project(v.clone());
        }

        let mut slots = self.design.extern_slots.iter().peekable();
        for pos in 0..=self.design.comb_eval_order.len() {
            while let Some(&&(_, id)) = slots.peek().filter(|s| s.0 == pos) {
                self.external(id)?;
                slots.next();
            }
            if let Some(id) = self.design.comb_eval_order.get(pos) {
                let a = &self.design.comb[id.0];
                let signal = self.design.signal(a.target.signal);
                let site = Site {
                    path: &signal.source.path,
                    span: a.span,
                };
                if let Some((id, low)) = self.target(&a.target, site)? {
                    let w = self.target_width(&a.target);
                    let v = self.eval(&a.value, w, site)?.resized(w);
                    self.assign(id, low, &v);
                }
            }
        }
        for (i, s) in self.design.signals.iter().enumerate() {
            if let Some(t) = &s.alias {
                self.values[i] = self.read_target(t, (&s.source).into())?;
            }
        }
        Ok(())
    }
    fn reset_active(&self, id: ResetId) -> bool {
        let r = &self.design.resets[id.0];
        matches!(
            (r.polarity, self.read(r.signal).truth()),
            (HwResetPolarity::ActiveHigh, Logic::One) | (HwResetPolarity::ActiveLow, Logic::Zero)
        )
    }
    pub fn eval_comb(&mut self) -> Result<()> {
        for _ in 0..=self.design.regs.len() {
            self.propagate()?;
            let mut updates = Vec::new();
            for r in &self.design.regs {
                if let Some((id, e)) = &r.reset
                    && self.design.resets[id.0].mode == HwResetMode::Async
                    && self.reset_active(*id)
                {
                    let value = self.eval(e, r.width, (&r.source).into())?.resized(r.width);
                    if value != self.values[r.signal.0] {
                        updates.push((r.signal, value))
                    }
                }
            }
            if updates.is_empty() {
                return Ok(());
            }
            for (id, v) in updates {
                self.values[id.0] = v;
            }
        }
        Err(RuntimeError::Invalid(
            "asynchronous reset logic did not settle".into(),
        ))
    }
    pub fn tick(&mut self, clock: &str) -> Result<()> {
        let id = self.clock(clock)?;
        self.step_clocks(&[ClockEvent::Rising(id)])?;
        self.step_clocks(&[ClockEvent::Falling(id)])
    }
    /// Sample every triggered process against the same pre-edge state.
    /// An assertion error aborts the group before any sequential write.
    pub fn step_clocks(&mut self, events: &[ClockEvent]) -> Result<()> {
        let mut seen = BTreeSet::new();
        let mut rising = BTreeSet::new();
        for &event in events {
            let id = event.domain();
            if id.0 >= self.design.clocks.len() || !seen.insert(self.clock_sources[id.0]) {
                return Err(RuntimeError::Invalid(format!(
                    "invalid or repeated clock event for {id:?}"
                )));
            }
            if matches!(event, ClockEvent::Rising(_)) {
                let source = self.clock_sources[id.0];
                rising.extend(
                    self.clock_sources
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| **s == source)
                        .map(|(i, _)| DomainId(i)),
                );
            }
        }
        self.eval_comb()?;
        let mut nets = Vec::new();
        let mut writes = Vec::new();
        for r in self
            .design
            .regs
            .iter()
            .filter(|r| rising.contains(&r.clock))
        {
            let e = r
                .reset
                .as_ref()
                .filter(|(id, _)| self.reset_active(*id))
                .map_or(&r.next, |(_, v)| v);
            let site = Site {
                path: &r.source.path,
                span: r.next_span,
            };
            nets.push((r.signal, 0, self.eval(e, r.width, site)?.resized(r.width)));
        }
        for (index, m) in self.design.memories.iter().enumerate() {
            let site = (&m.source).into();
            for p in m.sync_reads.iter().filter(|p| rising.contains(&p.clock)) {
                if self.eval(&p.enable, 0, site)?.truth() == Logic::One
                    && let Some((id, low)) = self.target(&p.target, site)?
                {
                    let v = self.read_word(MemId(index), &p.address, site)?;
                    nets.push((
                        id,
                        low,
                        self.project(v.resized(self.target_width(&p.target))),
                    ));
                }
            }
            for p in m.writes.iter().filter(|p| rising.contains(&p.clock)) {
                if self.eval(&p.enable, 0, site)?.truth() == Logic::One
                    && let Some(addr) = self
                        .eval(&p.address, 0, site)?
                        .as_u64()
                        .filter(|a| *a < u64::from(m.depth))
                {
                    writes.push((
                        index,
                        addr as usize,
                        self.eval(&p.data, m.width, site)?.resized(m.width),
                    ));
                }
            }
        }
        for a in self
            .design
            .assertions
            .iter()
            .filter(|a| rising.contains(&a.clock))
        {
            if !a.reset_mask.is_some_and(|r| self.reset_active(r))
                && self.eval(&a.condition, 0, (&a.source).into())?.truth() == Logic::Zero
            {
                return Err(AssertionFailure {
                    path: a.source.path.clone(),
                    span: a.source.span(),
                    cycle: self.cycle + 1,
                    message: a.message.clone(),
                }
                .into());
            }
        }
        for (id, low, v) in nets {
            self.values[id.0].insert(low, &v)
        }
        for (id, addr, v) in writes {
            self.memories[id][addr] = v
        }
        for &event in events {
            let clock = self.clock_sources[event.domain().0];
            self.values[clock.0] =
                Value::from_u64(1, matches!(event, ClockEvent::Rising(_)).into());
        }
        self.cycle += u64::from(!rising.is_empty());
        self.eval_comb()
    }
}
