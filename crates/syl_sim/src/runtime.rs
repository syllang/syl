//! Event-driven reference execution of a concrete design.
mod trajectory;
pub(crate) use trajectory::TrajectoryState;
mod expr;
mod format;
use crate::*;
pub(crate) use expr::signed;
use std::collections::{BTreeMap, BTreeSet};
use std::ops::ControlFlow;
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
    pub assert_id: AssertId,
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
type PortValues = BTreeMap<String, Value>;
#[derive(Clone)]
struct InoutNet {
    pin: Value,
    resolved: Value,
}
struct External {
    clocks: Vec<String>,
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

pub(crate) enum AssertionMode {
    Stop,
    Record(Option<(u64, AssertId)>),
    Replay { step: u64, id: AssertId },
}

type LogSink<'d> = Box<dyn FnMut(&str) + 'd>;

pub struct Simulator<'d> {
    log_sink: Option<LogSink<'d>>,
    pub log_lines: Vec<String>,
    design: &'d SimDesign,
    mode: StateMode,
    trajectory: TrajectoryState,
    values: Vec<Value>,
    models: BTreeMap<InstanceId, External>,
    pub(crate) assertions: AssertionMode,
    pub(crate) stimulus_step: Option<u64>,
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

// truth() normalizes a high-impedance bit to X.
fn posedge(old: Logic, new: Logic) -> bool {
    matches!(
        (old, new),
        (Logic::Zero, Logic::One | Logic::X) | (Logic::X, Logic::One)
    )
}

pub(crate) fn clock_sources(design: &SimDesign) -> Vec<SignalId> {
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
    design
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
        .collect()
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

        let clock_sources = clock_sources(design);
        Self {
            design,
            mode,
            values: design.signals.iter().map(|s| initial(s.width)).collect(),
            models: BTreeMap::new(),
            trajectory: TrajectoryState {
                clock_sources,
                last_clocks: vec![Logic::X; design.clocks.len()],
                last_resets: vec![Logic::X; design.resets.len()],
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
                    .flat_map(|p| {
                        std::iter::once(p.signal)
                            .chain(design.signal(p.signal).alias.as_ref().map(|a| a.signal))
                    })
                    .map(|id| {
                        let released = Value::z(design.signal(id).width);
                        (
                            id,
                            InoutNet {
                                pin: released.clone(),
                                resolved: released,
                            },
                        )
                    })
                    .collect(),
                cycle: 0,
            },
            log_sink: None,
            log_lines: Vec::new(),
            assertions: AssertionMode::Stop,
            stimulus_step: None,
            strict_unique: false,
            unique_violations: Vec::new(),
        }
    }
    /// Stream each sampled line to the callback instead of collecting it in log_lines.
    pub fn set_log_sink(&mut self, sink: impl FnMut(&str) + 'd) {
        self.log_sink = Some(Box::new(sink));
    }
    pub fn design(&self) -> &'d SimDesign {
        self.design
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
        self.trajectory.cycle
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
        self.trajectory
            .clock_sources
            .iter()
            .position(|s| *s == signal)
            .map(DomainId)
            .ok_or_else(|| RuntimeError::Invalid(format!("{name} is not a clock")))
    }
    /// Drive one input. Call eval_comb after a batch of simultaneous drives.
    pub fn drive(&mut self, id: SignalId, value: Value) -> Result<()> {
        self.values[id.0] = self.trajectory.drive(self.design, self.mode, id, value)?;
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
        self.trajectory.memory(id, address)
    }
    pub fn register_extern(
        &mut self,
        id: InstanceId,
        model: impl ExternModel + 'static,
    ) -> Result<()> {
        let Some(external) = self.design.externs.iter().find(|e| e.instance == id) else {
            return Err(RuntimeError::Invalid(format!(
                "{id:?} is not an extern instance"
            )));
        };
        self.models.insert(
            id,
            External {
                clocks: self.design.instances[id.0]
                    .ports
                    .iter()
                    .filter(|p| external.clocks.contains(&p.signal))
                    .map(|p| p.name.clone())
                    .collect(),
                model: Box::new(model),
                previous: None,
                outputs: Vec::new(),
            },
        );
        Ok(())
    }
    // The low overlapping bits are electrically connected. Unconnected
    // formal bits resolve locally, including the formal's own drivers.
    fn net_value(&mut self, id: SignalId, site: Site) -> Result<Value> {
        let mut value = self.values[id.0].clone();
        if let Some(actual) = &self.design.signal(id).alias {
            let parent = self.read_target(actual, site)?;
            let width = parent.width().min(value.width());
            value.insert(0, &parent.resized(width));
        }
        Ok(value)
    }
    fn external(
        &mut self,
        id: InstanceId,
        deferred: &mut Vec<(InstanceId, PortValues)>,
    ) -> Result<bool> {
        let inst = &self.design.instances[id.0];
        let bidirectional = inst.ports.iter().any(|p| p.direction == HwDirection::InOut);
        let entry = self.models.get(&id).ok_or_else(|| {
            RuntimeError::Invalid(format!("unregistered extern cell {}", inst.source.path))
        })?;
        // A bidirectional callback samples the resolved pins, including its own
        // standing drives. If it changes those drives, settle again before
        // visiting consumers or sampling any sequential process.
        if bidirectional {
            for (net, v) in entry.outputs.clone() {
                self.drive_net(net, 0, &v, (&inst.source).into())?;
            }
        }
        let mut inputs = BTreeMap::new();
        for p in inst
            .ports
            .iter()
            .filter(|p| p.direction != HwDirection::Out)
        {
            let value = self.net_value(p.signal, (&inst.source).into())?;
            inputs.insert(p.name.clone(), value);
        }
        let entry = &self.models[&id];
        if entry.previous.as_ref() != Some(&inputs) {
            let edge = entry.previous.as_ref().is_some_and(|previous| {
                entry
                    .clocks
                    .iter()
                    .any(|name| previous[name] != inputs[name])
            });
            if edge {
                deferred.push((id, inputs));
            } else if self.evaluate_model(id, inputs)? && bidirectional {
                return Ok(false);
            }
        }
        if !bidirectional {
            for (net, v) in self.models[&id].outputs.clone() {
                self.drive_net(net, 0, &v, (&inst.source).into())?;
            }
        }
        Ok(true)
    }
    fn evaluate_model(&mut self, id: InstanceId, inputs: PortValues) -> Result<bool> {
        let inst = &self.design.instances[id.0];
        let path = &inst.source.path;
        let entry = self.models.get_mut(&id).expect("registered model");
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
        let changed = entry.outputs != drives;
        entry.outputs = drives;
        entry.previous = Some(inputs);
        Ok(changed)
    }
    fn drive_net(&mut self, id: SignalId, low: u32, v: &Value, site: Site) -> Result<()> {
        self.assign(id, low, v);
        if let Some(actual) = &self.design.signal(id).alias {
            let connected = self.target_width(actual).min(self.design.signal(id).width);
            if low < connected
                && let Some((parent, offset)) = self.target(actual, site)?
            {
                let width = v.width().min(connected - low);
                self.drive_net(parent, offset + low, &v.resized(width), site)?;
            }
        }
        Ok(())
    }
    fn assign(&mut self, id: SignalId, low: u32, v: &Value) {
        if let Some(net) = self.trajectory.inout_drives.get_mut(&id) {
            net.resolved.resolve(low, v);
            let resolved = net.resolved.clone();
            self.values[id.0] = self.project(resolved);
        } else {
            let v = self.project(v.clone());
            self.values[id.0].insert(low, &v)
        }
    }
    fn propagate_once(&mut self) -> Result<ControlFlow<InstanceId, Vec<(InstanceId, PortValues)>>> {
        let mut deferred = Vec::new();
        for (id, net) in &mut self.trajectory.inout_drives {
            net.resolved.clone_from(&net.pin);
            self.values[id.0] = if self.mode == StateMode::TwoState {
                net.pin.clone().into_two_state()
            } else {
                net.pin.clone()
            };
        }
        let mut slots = self.design.extern_slots.iter().peekable();
        for pos in 0..=self.design.comb_eval_order.len() {
            while let Some(&&(_, id)) = slots.peek().filter(|s| s.0 == pos) {
                if !self.external(id, &mut deferred)? {
                    return Ok(ControlFlow::Break(id));
                }
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
                    let mode = if self.trajectory.inout_drives.contains_key(&id) {
                        StateMode::FourState
                    } else {
                        self.mode
                    };
                    let v = self.eval_as(&a.value, w, site, mode)?.resized(w);
                    self.drive_net(id, low, &v, site)?;
                }
            }
        }
        for (i, s) in self.design.signals.iter().enumerate() {
            if s.alias.is_some() {
                let v = self.net_value(SignalId(i), (&s.source).into())?;
                self.values[i] = self.project(v);
            }
        }
        Ok(ControlFlow::Continue(deferred))
    }
    fn propagate(&mut self) -> Result<Vec<(InstanceId, PortValues)>> {
        // The scheduler visits producers first. Until the current inout model
        // settles, its upstream drives and all clocked state stay fixed.
        // Brent's cycle detection retains one set of drives, even for a ripple
        // across a wide bus, and invokes the model just once per delta.
        let mut checkpoint = None;
        let mut interval = 1usize;
        let mut remaining = 0;
        loop {
            let id = match self.propagate_once()? {
                ControlFlow::Continue(deferred) => return Ok(deferred),
                ControlFlow::Break(id) => id,
            };
            let drives = &self.models[&id].outputs;
            if let Some((prior, saved)) = &checkpoint
                && *prior == id
            {
                if drives == saved {
                    let path = &self.design.instances[id.0].source.path;
                    return Err(RuntimeError::Invalid(format!(
                        "extern {path} did not settle: its combinational drives repeat"
                    )));
                }
                remaining -= 1;
                if remaining > 0 {
                    continue;
                }
                interval = interval.saturating_mul(2);
            } else {
                interval = 1;
            }
            checkpoint = Some((id, drives.clone()));
            remaining = interval;
        }
    }
    fn reset_level(&self, id: ResetId) -> Logic {
        let r = &self.design.resets[id.0];
        match (r.polarity, self.read(r.signal).truth()) {
            (HwResetPolarity::ActiveLow, Logic::One) => Logic::Zero,
            (HwResetPolarity::ActiveLow, Logic::Zero) => Logic::One,
            (_, level) => level,
        }
    }
    /// Resolve combinational nets for the replay baseline before any clock event.
    pub(crate) fn settle_initial_pins(&mut self) -> Result<()> {
        self.propagate().map(|_| ())
    }
    pub fn eval_comb(&mut self) -> Result<()> {
        // A sequential commit may itself change a generated clock or reset.
        // Finish each delta's sampling before propagating the next delta.
        let limit = 2
            * (self.design.clocks.len() + self.design.resets.len() + self.design.externs.len() + 1);
        for _ in 0..limit {
            let deferred = self.propagate()?;
            let clocks: Vec<_> = self
                .design
                .clocks
                .iter()
                .map(|c| self.read(c.signal).truth())
                .collect();
            let resets: Vec<_> = (0..self.design.resets.len())
                .map(|i| self.reset_level(ResetId(i)))
                .collect();
            let rising: BTreeSet<_> = self
                .trajectory
                .last_clocks
                .iter()
                .zip(&clocks)
                .enumerate()
                .filter_map(|(i, (&old, &new))| posedge(old, new).then_some(DomainId(i)))
                .collect();
            let reset_edges: BTreeSet<_> = self
                .trajectory
                .last_resets
                .iter()
                .zip(&resets)
                .enumerate()
                .filter_map(|(i, (&old, &new))| {
                    (posedge(old, new) && self.design.resets[i].mode == HwResetMode::Async)
                        .then_some(ResetId(i))
                })
                .collect();
            if rising.is_empty() && reset_edges.is_empty() && deferred.is_empty() {
                self.trajectory.last_clocks = clocks;
                self.trajectory.last_resets = resets;
                return Ok(());
            }
            self.commit_group(&rising, &reset_edges, deferred)?;
            self.trajectory.last_clocks = clocks;
            self.trajectory.last_resets = resets;
            self.trajectory.cycle += u64::from(!rising.is_empty());
        }
        Err(RuntimeError::Invalid(
            "clock/reset or extern propagation did not settle".into(),
        ))
    }
    pub fn tick(&mut self, clock: &str) -> Result<()> {
        let id = self.clock(clock)?;
        let source = self.trajectory.clock_sources[id.0];
        if self.read(source).truth() != Logic::Zero {
            self.step_clocks(&[ClockEvent::Falling(id)])?;
        }
        self.step_clocks(&[ClockEvent::Rising(id)])?;
        self.step_clocks(&[ClockEvent::Falling(id)])
    }
    /// Apply simultaneous clock transitions and settle their derived events.
    pub fn step_clocks(&mut self, events: &[ClockEvent]) -> Result<()> {
        let mut seen = BTreeSet::new();
        for &event in events {
            let id = event.domain();
            if id.0 >= self.trajectory.clock_sources.len()
                || !seen.insert(self.trajectory.clock_sources[id.0])
            {
                return Err(RuntimeError::Invalid(format!(
                    "invalid or repeated clock event for {id:?}"
                )));
            }
        }
        self.eval_comb()?;
        for &event in events {
            let signal = self.trajectory.clock_sources[event.domain().0];
            self.values[signal.0] =
                Value::from_u64(1, matches!(event, ClockEvent::Rising(_)).into());
        }
        self.eval_comb()
    }
    fn commit_group(
        &mut self,
        rising: &BTreeSet<DomainId>,
        reset_edges: &BTreeSet<ResetId>,
        deferred: Vec<(InstanceId, PortValues)>,
    ) -> Result<()> {
        let mut nets = Vec::new();
        let mut writes = Vec::new();
        for r in self.design.regs.iter().filter(|r| {
            rising.contains(&r.clock)
                || r.reset
                    .as_ref()
                    .is_some_and(|(id, _)| reset_edges.contains(id))
        }) {
            let e = r
                .reset
                .as_ref()
                .filter(|(id, _)| self.reset_level(*id) == Logic::One)
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
        if !matches!(self.assertions, AssertionMode::Record(_)) {
            self.sample_logs(rising)?;
        }
        for (id, a) in self
            .design
            .assertions
            .iter()
            .enumerate()
            .filter(|(_, a)| rising.contains(&a.clock))
        {
            if a.reset_mask
                .is_none_or(|r| self.reset_level(r) == Logic::Zero)
                && self.eval(&a.condition, 0, (&a.source).into())?.truth() == Logic::Zero
            {
                let cycle = self.stimulus_step.unwrap_or_else(|| self.cycle() + 1);
                let coordinate = (cycle, AssertId(id));
                match &mut self.assertions {
                    AssertionMode::Record(first) => {
                        if first.is_none_or(|previous| coordinate < previous) {
                            *first = Some(coordinate);
                        }
                        break;
                    }
                    // A larger ID may fail during an earlier transition of this
                    // same stimulus step. Continue until the selected site fires.
                    AssertionMode::Replay { step, id } if cycle == *step && coordinate.1 > *id => {
                        break;
                    }
                    _ => {}
                }
                return Err(AssertionFailure {
                    assert_id: AssertId(id),
                    path: a.source.path.clone(),
                    span: a.source.span(),
                    cycle,
                    message: self.format_message(&a.message, (&a.source).into())?,
                }
                .into());
            }
        }
        for (id, inputs) in deferred {
            self.evaluate_model(id, inputs)?;
        }
        for (id, low, v) in nets {
            self.values[id.0].insert(low, &v)
        }
        for (id, addr, v) in writes {
            self.trajectory.memories[id][addr] = v
        }
        Ok(())
    }
}
