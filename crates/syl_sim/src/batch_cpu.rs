//! A packed two-state CPU batch evaluator.
//!
//! The state plane is laid out as `slot * batch_size + lane`. Evaluation walks
//! the concrete design schedule directly, so no event-driven `Simulator` is
//! created for an individual lane.

use crate::{
    AssertionFailure, ClockEvent, HwBinaryOp, HwDirection, HwResetMode, HwResetPolarity, Logic,
    MemId, RuntimeError, Selector, SignalId, SimDesign, SimExpr, SimTarget, StateMode, Value,
};

#[derive(Debug, thiserror::Error)]
#[error("batch {batch_idx}, cycle {cycle}: {error}")]
pub struct BatchFailure {
    pub batch_idx: usize,
    pub cycle: u64,
    #[source]
    pub error: RuntimeError,
}

/// Independent 2-state trajectories backed by a structure-of-arrays signal plane.
pub struct BatchCpuSimulator<'d> {
    design: &'d SimDesign,
    /// Signal values are indexed by `signal * batch_size + lane`.
    state: Vec<Value>,
    /// Memory values are indexed by `memory_offsets[memory] + address * stride + lane`.
    memories: Vec<Value>,
    memory_offsets: Vec<usize>,
    batch_size: usize,
    last_clocks: Vec<Logic>,
    last_resets: Vec<Logic>,
    cycles: Vec<u64>,
    mode: StateMode,
}

impl<'d> BatchCpuSimulator<'d> {
    pub fn new(design: &'d SimDesign, batch_size: usize) -> Self {
        Self::with_mode(design, batch_size, StateMode::TwoState)
    }

    pub fn with_mode(design: &'d SimDesign, batch_size: usize, mode: StateMode) -> Self {
        let state = design
            .signals
            .iter()
            .flat_map(|s| (0..batch_size).map(move |_| Value::zero(s.width)))
            .collect();
        let mut memories = Vec::new();
        let mut memory_offsets = Vec::with_capacity(design.memories.len());
        for memory in &design.memories {
            memory_offsets.push(memories.len());
            memories.extend(
                (0..memory.depth as usize)
                    .flat_map(|_| (0..batch_size).map(move |_| Value::zero(memory.width))),
            );
        }
        Self {
            design,
            state,
            memories,
            memory_offsets,
            batch_size,
            last_clocks: vec![Logic::Zero; design.clocks.len() * batch_size],
            last_resets: vec![Logic::Zero; design.resets.len() * batch_size],
            cycles: vec![0; batch_size],
            mode,
        }
    }

    pub fn batch_size(&self) -> usize {
        self.batch_size
    }
    pub fn slot_count(&self) -> usize {
        self.design.signals.len()
    }
    pub fn batch_stride(&self) -> usize {
        self.batch_size
    }
    pub fn design(&self) -> &'d SimDesign {
        self.design
    }
    pub fn cycle(&self) -> u64 {
        self.cycles.first().copied().unwrap_or(0)
    }

    pub fn signal_state(&self, signal: SignalId, lane: usize) -> Option<&Value> {
        (lane < self.batch_size).then(|| &self.state[signal.0 * self.batch_size + lane])
    }
    pub fn read(&self, lane: usize, signal: SignalId) -> Option<&Value> {
        self.signal_state(signal, lane)
    }
    pub fn read_named(&self, lane: usize, name: &str) -> Result<&Value, RuntimeError> {
        self.check_lane(lane)?;
        let id = self.port(name)?;
        Ok(&self.state[id.0 * self.batch_size + lane])
    }
    pub fn read_batch(&self, signal: SignalId) -> Vec<Value> {
        (0..self.batch_size)
            .map(|lane| self.state[signal.0 * self.batch_size + lane].clone())
            .collect()
    }
    pub fn memory(&self, lane: usize, memory: MemId, address: u32) -> Option<&Value> {
        let m = self.design.memories.get(memory.0)?;
        if lane >= self.batch_size || address >= m.depth {
            return None;
        }
        Some(
            &self.memories
                [self.memory_offsets[memory.0] + address as usize * self.batch_size + lane],
        )
    }

    fn check_lane(&self, lane: usize) -> Result<(), RuntimeError> {
        if lane < self.batch_size {
            Ok(())
        } else {
            Err(RuntimeError::Invalid(format!(
                "batch lane {lane} is out of range"
            )))
        }
    }
    fn port(&self, name: &str) -> Result<SignalId, RuntimeError> {
        self.design
            .top_ports()
            .iter()
            .find(|p| p.name == name)
            .map(|p| p.signal)
            .ok_or_else(|| RuntimeError::Invalid(format!("unknown top port {name}")))
    }
    fn fail(lane: usize, cycle: u64, error: RuntimeError) -> BatchFailure {
        BatchFailure {
            batch_idx: lane,
            cycle,
            error,
        }
    }
    fn project(&self, value: Value) -> Value {
        if self.mode == StateMode::TwoState {
            value.into_two_state()
        } else {
            value
        }
    }

    pub fn drive(
        &mut self,
        lane: usize,
        signal: SignalId,
        value: Value,
    ) -> Result<(), RuntimeError> {
        self.check_lane(lane)?;
        if !self
            .design
            .top_ports()
            .iter()
            .any(|p| p.signal == signal && p.direction != HwDirection::Out)
        {
            return Err(RuntimeError::Invalid(format!(
                "{signal:?} is not a top input"
            )));
        }
        let width = self.design.signal(signal).width;
        self.state[signal.0 * self.batch_size + lane] = self.project(value.resized(width));
        Ok(())
    }
    pub fn drive_named(
        &mut self,
        lane: usize,
        name: &str,
        value: Value,
    ) -> Result<(), RuntimeError> {
        let id = self.port(name)?;
        self.drive(lane, id, value)
    }
    pub fn broadcast(&mut self, signal: SignalId, value: Value) -> Result<(), RuntimeError> {
        for lane in 0..self.batch_size {
            self.drive(lane, signal, value.clone())?;
        }
        Ok(())
    }
    pub fn broadcast_named(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        let id = self.port(name)?;
        self.broadcast(id, value)
    }
    pub fn drive_batch(&mut self, signal: SignalId, values: &[Value]) -> Result<(), RuntimeError> {
        if values.len() != self.batch_size {
            return Err(RuntimeError::Invalid(format!(
                "drive batch has {} values, expected {}",
                values.len(),
                self.batch_size
            )));
        }
        for (lane, value) in values.iter().enumerate() {
            self.drive(lane, signal, value.clone())?;
        }
        Ok(())
    }

    fn target_width(&self, target: &SimTarget) -> u32 {
        target
            .selectors
            .last()
            .map_or(self.design.signal(target.signal).width, Selector::width)
    }
    fn natural(&self, e: &SimExpr) -> Result<u32, RuntimeError> {
        Ok(match e {
            SimExpr::Read(t) => self.target_width(t),
            SimExpr::Int(n) => (65 - n.leading_zeros()).max(32),
            SimExpr::Bool(_) | SimExpr::HighZ | SimExpr::Zero => 1,
            SimExpr::Unary { expr, .. } => self.natural(expr)?,
            SimExpr::Binary { op, left, right }
                if matches!(
                    op,
                    HwBinaryOp::Eq
                        | HwBinaryOp::NotEq
                        | HwBinaryOp::Lt
                        | HwBinaryOp::LtEq
                        | HwBinaryOp::Gt
                        | HwBinaryOp::GtEq
                        | HwBinaryOp::AndAnd
                        | HwBinaryOp::OrOr
                ) =>
            {
                if matches!(op, HwBinaryOp::AndAnd | HwBinaryOp::OrOr) {
                    1
                } else {
                    self.natural(left)?.max(self.natural(right)?)
                }
            }
            SimExpr::Binary {
                op: HwBinaryOp::Shl,
                left,
                ..
            } => self.natural(left)?,
            SimExpr::Binary { left, right, .. } => self.natural(left)?.max(self.natural(right)?),
            SimExpr::Mux {
                then_value,
                else_value,
                ..
            } => self.natural(then_value)?.max(self.natural(else_value)?),
            SimExpr::Select { arms, default, .. } => {
                arms.iter().try_fold(self.natural(default)?, |w, (_, v)| {
                    Ok::<u32, RuntimeError>(w.max(self.natural(v)?))
                })?
            }
            SimExpr::Concat(parts) => parts.iter().try_fold(0u32, |w, p| {
                w.checked_add(self.natural(p)?)
                    .ok_or_else(|| RuntimeError::Invalid("concatenation too wide".into()))
            })?,
            SimExpr::Project { selector, .. } => selector.width(),
            SimExpr::MemoryRead { memory, .. } => self.design.memories[memory.0].width,
        })
    }
    fn selector_offset(
        &mut self,
        selector: &Selector,
        parent: u32,
        lane: usize,
    ) -> Result<Option<u32>, RuntimeError> {
        let low = match selector {
            Selector::Slice { low, .. } => Some(*low),
            Selector::Index { index, width } => self
                .eval_expr(index, lane, 0)?
                .as_u64()
                .and_then(|v| u32::try_from(v).ok())
                .and_then(|v| v.checked_mul(*width)),
        };
        Ok(low.filter(|v| {
            v.checked_add(selector.width())
                .is_some_and(|end| end <= parent)
        }))
    }
    fn read_signal(&mut self, signal: SignalId, lane: usize) -> Result<Value, RuntimeError> {
        if let Some(alias) = self.design.signal(signal).alias.clone() {
            return self.read_target(&alias, lane);
        }
        Ok(self.state[signal.0 * self.batch_size + lane].clone())
    }
    fn read_target(&mut self, target: &SimTarget, lane: usize) -> Result<Value, RuntimeError> {
        let value = self.read_signal(target.signal, lane)?;
        let mut parent = value.width();
        let mut low = 0;
        for selector in target.selectors.clone() {
            let Some(offset) = self.selector_offset(&selector, parent, lane)? else {
                return Ok(Value::zero(selector.width()));
            };
            low += offset;
            parent = selector.width();
        }
        if target.selectors.is_empty() {
            Ok(value)
        } else {
            Ok(value.slice(low, parent))
        }
    }
    fn write_target(
        &mut self,
        target: &SimTarget,
        lane: usize,
        value: &Value,
    ) -> Result<(), RuntimeError> {
        let width = self.target_width(target);
        let mut low = 0;
        let mut parent = self.design.signal(target.signal).width;
        for selector in target.selectors.clone() {
            let Some(offset) = self.selector_offset(&selector, parent, lane)? else {
                return Ok(());
            };
            low += offset;
            parent = selector.width();
        }
        let id = target.signal;
        let mut updated = self.state[id.0 * self.batch_size + lane].clone();
        updated.insert(low, &value.resized(width));
        self.state[id.0 * self.batch_size + lane] = self.project(updated);
        if let Some(alias) = self.design.signal(id).alias.clone() {
            self.write_target(&alias, lane, value)?;
        }
        Ok(())
    }
    fn eval_expr(&mut self, e: &SimExpr, lane: usize, context: u32) -> Result<Value, RuntimeError> {
        let width = context.max(self.natural(e)?);
        let value = match e {
            SimExpr::Read(target) => self.read_target(target, lane)?,
            SimExpr::Int(n) => Value::from_u64(width, *n),
            SimExpr::Bool(v) => Value::from_u64(width, u64::from(*v)),
            SimExpr::HighZ | SimExpr::Zero => Value::zero(width),
            SimExpr::MemoryRead { memory, port } => {
                let address_expr = self.design.memories[memory.0].comb_reads[*port]
                    .address
                    .clone();
                let address = self.eval_expr(&address_expr, lane, 0)?.as_u64();
                address
                    .and_then(|a| {
                        (a < self.design.memories[memory.0].depth as u64).then(|| {
                            self.memories[self.memory_offsets[memory.0]
                                + a as usize * self.batch_size
                                + lane]
                                .clone()
                        })
                    })
                    .unwrap_or_else(|| Value::zero(self.design.memories[memory.0].width))
            }
            SimExpr::Unary { op, expr } => match op {
                crate::HwUnaryOp::Not => Value::from_u64(
                    1,
                    u64::from(self.eval_expr(expr, lane, 0)?.truth() == Logic::Zero),
                ),
                crate::HwUnaryOp::Neg => Value::zero(width)
                    .binary(HwBinaryOp::Sub, &self.eval_expr(expr, lane, width)?)
                    .map_err(RuntimeError::Invalid)?,
                _ => {
                    return Err(RuntimeError::Invalid(format!(
                        "unsupported unary operator {op:?}"
                    )));
                }
            },
            SimExpr::Binary { op, left, right } => {
                let operand_width = if matches!(op, HwBinaryOp::AndAnd | HwBinaryOp::OrOr) {
                    0
                } else if matches!(
                    op,
                    HwBinaryOp::Eq
                        | HwBinaryOp::NotEq
                        | HwBinaryOp::Lt
                        | HwBinaryOp::LtEq
                        | HwBinaryOp::Gt
                        | HwBinaryOp::GtEq
                ) {
                    self.natural(left)?.max(self.natural(right)?)
                } else {
                    width
                };
                let a = self.eval_expr(left, lane, operand_width)?;
                let b = self.eval_expr(
                    right,
                    lane,
                    if matches!(op, HwBinaryOp::Shl | HwBinaryOp::AndAnd | HwBinaryOp::OrOr) {
                        0
                    } else {
                        operand_width
                    },
                )?;
                a.binary(*op, &b).map_err(RuntimeError::Invalid)?
            }
            SimExpr::Mux {
                cond,
                then_value,
                else_value,
            } => {
                if self.eval_expr(cond, lane, 0)?.truth() == Logic::One {
                    self.eval_expr(then_value, lane, width)?
                } else {
                    self.eval_expr(else_value, lane, width)?
                }
            }
            SimExpr::Select { arms, default, .. } => {
                let mut result = self.eval_expr(default, lane, width)?;
                for (guard, arm) in arms.iter().rev() {
                    if self.eval_expr(guard, lane, 0)?.truth() == Logic::One {
                        result = self.eval_expr(arm, lane, width)?;
                    }
                }
                result
            }
            SimExpr::Concat(parts) => {
                let values = parts
                    .iter()
                    .map(|p| self.eval_expr(p, lane, 0))
                    .collect::<Result<Vec<_>, _>>()?;
                Value::concat(&values)
                    .ok_or_else(|| RuntimeError::Invalid("concatenation too wide".into()))?
            }
            SimExpr::Project { value, selector } => {
                let value = self.eval_expr(value, lane, 0)?;
                let Some(offset) = self.selector_offset(selector, value.width(), lane)? else {
                    return Ok(Value::zero(selector.width()));
                };
                value.slice(offset, selector.width())
            }
        };
        Ok(self.project(value.resized(width)))
    }

    fn eval_comb_lane(&mut self, lane: usize) -> Result<(), RuntimeError> {
        for comb_id in self.design.comb_eval_order.clone() {
            let assign = self.design.comb[comb_id.0].clone();
            let value = self.eval_expr(&assign.value, lane, self.target_width(&assign.target))?;
            self.write_target(&assign.target, lane, &value)?;
        }
        Ok(())
    }
    fn reset_level(&self, id: usize, lane: usize) -> Logic {
        let r = &self.design.resets[id];
        let truth = self.state[r.signal.0 * self.batch_size + lane].truth();
        match (r.polarity, truth) {
            (HwResetPolarity::ActiveLow, Logic::One) => Logic::Zero,
            (HwResetPolarity::ActiveLow, Logic::Zero) => Logic::One,
            (_, level) => level,
        }
    }
    fn commit(
        &mut self,
        lane: usize,
        rising: &[usize],
        reset_edges: &[usize],
    ) -> Result<(), RuntimeError> {
        let mut regs = Vec::new();
        for reg in &self.design.regs {
            if rising.contains(&reg.clock.0)
                || reg
                    .reset
                    .as_ref()
                    .is_some_and(|(r, _)| reset_edges.contains(&r.0))
            {
                let value = if let Some((reset, expr)) = &reg.reset
                    && self.reset_level(reset.0, lane) == Logic::One
                {
                    self.eval_expr(expr, lane, reg.width)?
                } else {
                    self.eval_expr(&reg.next, lane, reg.width)?
                };
                regs.push((reg.signal, value.resized(reg.width)));
            }
        }
        let mut reads = Vec::new();
        let mut writes = Vec::new();
        for (memory_id, memory) in self.design.memories.iter().enumerate() {
            for port in memory
                .sync_reads
                .iter()
                .filter(|p| rising.contains(&p.clock.0))
            {
                if self.eval_expr(&port.enable, lane, 0)?.truth() == Logic::One {
                    let address = self.eval_expr(&port.address, lane, 0)?.as_u64();
                    let value = address
                        .and_then(|a| {
                            (a < memory.depth as u64).then(|| {
                                self.memories[self.memory_offsets[memory_id]
                                    + a as usize * self.batch_size
                                    + lane]
                                    .clone()
                            })
                        })
                        .unwrap_or_else(|| Value::zero(memory.width));
                    reads.push((port.target.clone(), value));
                }
            }
            for port in memory.writes.iter().filter(|p| rising.contains(&p.clock.0)) {
                if self.eval_expr(&port.enable, lane, 0)?.truth() == Logic::One
                    && let Some(address) = self
                        .eval_expr(&port.address, lane, 0)?
                        .as_u64()
                        .filter(|a| *a < memory.depth as u64)
                {
                    writes.push((
                        memory_id,
                        address as usize,
                        self.eval_expr(&port.data, lane, memory.width)?,
                    ));
                }
            }
        }
        for assertion in &self.design.assertions {
            if rising.contains(&assertion.clock.0)
                && assertion
                    .reset_mask
                    .is_none_or(|r| self.reset_level(r.0, lane) == Logic::Zero)
                && self.eval_expr(&assertion.condition, lane, 0)?.truth() == Logic::Zero
            {
                return Err(AssertionFailure {
                    path: assertion.source.path.clone(),
                    span: assertion.source.span(),
                    cycle: self.cycles[lane] + 1,
                    message: assertion.message.clone(),
                }
                .into());
            }
        }
        for (signal, value) in regs {
            self.write_target(
                &SimTarget {
                    signal,
                    selectors: Vec::new(),
                },
                lane,
                &value,
            )?;
        }
        for (target, value) in reads {
            self.write_target(&target, lane, &value)?;
        }
        for (memory, address, value) in writes {
            self.memories[self.memory_offsets[memory] + address * self.batch_size + lane] = value;
        }
        Ok(())
    }
    fn settle_lane(&mut self, lane: usize) -> Result<(), RuntimeError> {
        self.eval_comb_lane(lane)?;
        let clocks: Vec<_> = self
            .design
            .clocks
            .iter()
            .map(|c| self.state[c.signal.0 * self.batch_size + lane].truth())
            .collect();
        let resets: Vec<_> = (0..self.design.resets.len())
            .map(|r| self.reset_level(r, lane))
            .collect();
        let rising: Vec<_> = clocks
            .iter()
            .enumerate()
            .filter_map(|(i, &new)| {
                (self.last_clocks[i * self.batch_size + lane] == Logic::Zero && new == Logic::One)
                    .then_some(i)
            })
            .collect();
        let reset_edges: Vec<_> = resets
            .iter()
            .enumerate()
            .filter_map(|(i, &new)| {
                (self.last_resets[i * self.batch_size + lane] == Logic::Zero
                    && new == Logic::One
                    && self.design.resets[i].mode == HwResetMode::Async)
                    .then_some(i)
            })
            .collect();
        if !rising.is_empty() || !reset_edges.is_empty() {
            self.commit(lane, &rising, &reset_edges)?;
            self.cycles[lane] += u64::from(!rising.is_empty());
        }
        for (i, clock) in clocks.into_iter().enumerate() {
            self.last_clocks[i * self.batch_size + lane] = clock;
        }
        for (i, reset) in resets.into_iter().enumerate() {
            self.last_resets[i * self.batch_size + lane] = reset;
        }
        Ok(())
    }
    pub fn eval_comb(&mut self) -> Result<(), BatchFailure> {
        for lane in 0..self.batch_size {
            let cycle = self.cycles[lane];
            if let Err(error) = self.settle_lane(lane) {
                return Err(Self::fail(lane, cycle, error));
            }
        }
        Ok(())
    }
    pub fn tick(&mut self, clock: &str) -> Result<(), BatchFailure> {
        let domain = self
            .design
            .clocks
            .iter()
            .position(|c| {
                c.name.as_deref() == Some(clock)
                    || self.design.signal(c.signal).source.name == clock
            })
            .ok_or_else(|| {
                Self::fail(
                    0,
                    0,
                    RuntimeError::Invalid(format!("unknown clock {clock}")),
                )
            })?;
        let signal = self.design.clocks[domain].signal;
        for lane in 0..self.batch_size {
            let cycle = self.cycles[lane];
            if self.state[signal.0 * self.batch_size + lane].truth() != Logic::Zero {
                self.state[signal.0 * self.batch_size + lane] = Value::zero(1);
                if let Err(error) = self.settle_lane(lane) {
                    return Err(Self::fail(lane, cycle, error));
                }
            }
            self.state[signal.0 * self.batch_size + lane] = Value::from_u64(1, 1);
            if let Err(error) = self.settle_lane(lane) {
                return Err(Self::fail(lane, cycle, error));
            }
            self.state[signal.0 * self.batch_size + lane] = Value::zero(1);
            if let Err(error) = self.settle_lane(lane) {
                return Err(Self::fail(lane, cycle, error));
            }
        }
        Ok(())
    }
    pub fn step_clocks(&mut self, events: &[ClockEvent]) -> Result<(), BatchFailure> {
        for event in events {
            let id = event_domain(*event);
            let name = self.design.clocks[id.0].name.clone().unwrap_or_else(|| {
                self.design
                    .signal(self.design.clocks[id.0].signal)
                    .source
                    .name
                    .clone()
            });
            self.tick(&name)?;
        }
        Ok(())
    }
}

fn event_domain(event: ClockEvent) -> crate::DomainId {
    match event {
        ClockEvent::Rising(id) | ClockEvent::Falling(id) => id,
    }
}
