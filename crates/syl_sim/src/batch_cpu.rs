//! Value-based batch execution with slot-major signals and T2 event semantics.
use crate::runtime::TrajectoryState;
use crate::{ClockEvent, MemId, RuntimeError, SignalId, SimDesign, Simulator, StateMode, Value};

#[derive(Debug, thiserror::Error)]
#[error("batch {batch_idx}, cycle {cycle}: {error}")]
pub struct BatchFailure {
    pub batch_idx: usize,
    pub cycle: u64,
    #[source]
    pub error: RuntimeError,
}

/// Independent trajectories with slot-major signals and lane-local RAM and clock history.
pub struct BatchCpuSimulator<'d> {
    design: &'d SimDesign,
    state: Vec<Value>,
    scratch: Vec<Value>,
    lanes: Vec<TrajectoryState>,
    mode: StateMode,
}

impl<'d> BatchCpuSimulator<'d> {
    pub fn new(design: &'d SimDesign, batch_size: usize) -> Self {
        Self::with_mode(design, batch_size, StateMode::TwoState)
    }

    pub fn with_mode(design: &'d SimDesign, batch_size: usize, mode: StateMode) -> Self {
        let (first, scratch) = TrajectoryState::new(design, mode);
        let state = scratch
            .iter()
            .flat_map(|value| std::iter::repeat_n(value.clone(), batch_size))
            .collect();
        let lanes = vec![first; batch_size];
        Self {
            design,
            mode,
            state,
            scratch,
            lanes,
        }
    }

    pub fn batch_size(&self) -> usize {
        self.lanes.len()
    }
    pub fn slot_count(&self) -> usize {
        self.design.signals.len()
    }
    pub fn batch_stride(&self) -> usize {
        self.lanes.len()
    }
    pub fn design(&self) -> &'d SimDesign {
        self.design
    }
    pub fn cycle(&self) -> u64 {
        self.lanes.first().map_or(0, TrajectoryState::cycle)
    }

    pub fn signal_state(&self, signal: SignalId, lane: usize) -> Option<&Value> {
        (lane < self.lanes.len()).then(|| &self.state[signal.0 * self.lanes.len() + lane])
    }
    pub fn read(&self, lane: usize, signal: SignalId) -> Option<&Value> {
        self.signal_state(signal, lane)
    }
    pub fn read_named(&self, lane: usize, name: &str) -> Result<&Value, RuntimeError> {
        self.check_lane(lane)?;
        let id = self.port(name)?;
        Ok(&self.state[id.0 * self.lanes.len() + lane])
    }
    pub fn read_batch(&self, signal: SignalId) -> Vec<Value> {
        (0..self.lanes.len())
            .map(|lane| self.state[signal.0 * self.lanes.len() + lane].clone())
            .collect()
    }
    pub fn memory(&self, lane: usize, memory: MemId, address: u32) -> Option<&Value> {
        self.lanes.get(lane)?.memory(memory, address)
    }

    fn check_lane(&self, lane: usize) -> Result<(), RuntimeError> {
        if lane < self.lanes.len() {
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
    pub fn drive(
        &mut self,
        lane: usize,
        signal: SignalId,
        value: Value,
    ) -> Result<(), RuntimeError> {
        self.check_lane(lane)?;
        let index = signal.0;
        let value = self.lanes[lane].drive(self.design, self.mode, signal, value)?;
        self.state[index * self.lanes.len() + lane] = value;
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
        for lane in 0..self.lanes.len() {
            self.drive(lane, signal, value.clone())?;
        }
        Ok(())
    }
    pub fn broadcast_named(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        let id = self.port(name)?;
        self.broadcast(id, value)
    }
    pub fn drive_batch(&mut self, signal: SignalId, values: &[Value]) -> Result<(), RuntimeError> {
        if values.len() != self.lanes.len() {
            return Err(RuntimeError::Invalid(format!(
                "drive batch has {} values, expected {}",
                values.len(),
                self.lanes.len()
            )));
        }
        for (lane, value) in values.iter().enumerate() {
            self.drive(lane, signal, value.clone())?;
        }
        Ok(())
    }

    fn advance(
        &mut self,
        operation: impl Fn(&mut Simulator<'_>) -> Result<(), RuntimeError>,
    ) -> Result<(), BatchFailure> {
        let stride = self.lanes.len();
        for (batch_idx, lane) in self.lanes.iter_mut().enumerate() {
            let mut exchange = |scratch: &mut [Value]| {
                for (slot, row) in scratch.iter_mut().zip(self.state.chunks_exact_mut(stride)) {
                    std::mem::swap(slot, &mut row[batch_idx]);
                }
            };
            exchange(&mut self.scratch);
            let result = lane.execute(self.design, self.mode, &mut self.scratch, &operation);
            exchange(&mut self.scratch);
            result.map_err(|error| {
                let cycle = match &error {
                    RuntimeError::Assertion(f) => f.cycle,
                    RuntimeError::UniqueOverlap(f) => f.cycle,
                    RuntimeError::Invalid(_) => lane.cycle(),
                };
                BatchFailure {
                    batch_idx,
                    cycle,
                    error,
                }
            })?;
        }
        Ok(())
    }

    pub fn eval_comb(&mut self) -> Result<(), BatchFailure> {
        self.advance(|s| s.eval_comb())
    }
    pub fn tick(&mut self, clock: &str) -> Result<(), BatchFailure> {
        self.advance(|s| s.tick(clock))
    }
    pub fn step_clocks(&mut self, events: &[ClockEvent]) -> Result<(), BatchFailure> {
        self.advance(|s| s.step_clocks(events))
    }
}
