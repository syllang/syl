//! Batch execution for independent two-state simulation lanes.
//!
//! The reference [`Simulator`](crate::Simulator) stores one value per signal.
//! A batch simulator keeps a second, packed view of those values with the
//! layout `signal * batch_size + lane`.  The packed view is useful to callers
//! that feed the same design to a CPU SIMD kernel, while the reference
//! scheduler remains the source of truth for event ordering and expression
//! semantics.  Keeping the two views in lockstep also makes this executor a
//! convenient conformance oracle for optimized kernels.

use crate::{MemId, RuntimeError, SignalId, SimDesign, Simulator, StateMode, Value};

/// The first lane and cycle at which a batch operation failed.
#[derive(Debug, thiserror::Error)]
#[error("batch {batch_idx}, cycle {cycle}: {error}")]
pub struct BatchFailure {
    /// Zero-based lane containing the failure.
    pub batch_idx: usize,
    /// Cycle observed while executing the failing operation.
    pub cycle: u64,
    /// The reference simulator error, including assertion diagnostics.
    #[source]
    pub error: RuntimeError,
}

/// A deterministic batch executor for independent two-state trajectories.
///
/// Each lane has independent registers and memories.  Drives are applied to a
/// selected lane, while [`tick`](Self::tick) and [`eval_comb`](Self::eval_comb)
/// process all lanes in ascending order.  This ordering makes the first
/// failure deterministic even when a caller uses a parallel outer loop.
pub struct BatchCpuSimulator<'d> {
    design: &'d SimDesign,
    lanes: Vec<Simulator<'d>>,
    /// Structure-of-arrays signal storage.  The index is `slot * stride + lane`.
    state: Vec<Value>,
    batch_size: usize,
}

impl<'d> BatchCpuSimulator<'d> {
    /// Construct `batch_size` independent two-state trajectories.
    pub fn new(design: &'d SimDesign, batch_size: usize) -> Self {
        Self::with_mode(design, batch_size, StateMode::TwoState)
    }

    /// Construct a batch with an explicit state mode.  Two-state mode is the
    /// intended fast path, while four-state mode is useful for diagnostics.
    pub fn with_mode(design: &'d SimDesign, batch_size: usize, mode: StateMode) -> Self {
        let lanes: Vec<_> = (0..batch_size)
            .map(|_| Simulator::with_mode(design, mode))
            .collect();
        let mut state = Vec::with_capacity(design.signals.len() * batch_size);
        for signal in 0..design.signals.len() {
            for lane in &lanes {
                state.push(lane.read(SignalId(signal)).clone());
            }
        }
        Self {
            design,
            lanes,
            state,
            batch_size,
        }
    }

    /// Number of independent trajectories in this batch.
    pub fn batch_size(&self) -> usize {
        self.batch_size
    }

    /// Number of signal slots in the packed state view.
    pub fn slot_count(&self) -> usize {
        self.design.signals.len()
    }

    /// Number of values between two adjacent signal slots.
    pub fn batch_stride(&self) -> usize {
        self.batch_size
    }

    /// Return a packed signal value without allocating.
    pub fn signal_state(&self, signal: SignalId, lane: usize) -> Option<&Value> {
        (lane < self.batch_size).then(|| &self.state[signal.0 * self.batch_size + lane])
    }

    /// Return a lane's public reference simulator.
    pub fn lane(&self, lane: usize) -> Option<&Simulator<'d>> {
        self.lanes.get(lane)
    }

    /// Return the design used by this batch.
    pub fn design(&self) -> &'d SimDesign {
        self.design
    }

    /// The committed cycle count.  All lanes advance in lockstep when using
    /// [`tick`](Self::tick).
    pub fn cycle(&self) -> u64 {
        self.lanes.first().map_or(0, Simulator::cycle)
    }

    fn check_lane(&self, lane: usize) -> Result<(), RuntimeError> {
        if lane < self.batch_size {
            Ok(())
        } else {
            Err(RuntimeError::Invalid(format!(
                "batch lane {lane} is outside 0..{}",
                self.batch_size
            )))
        }
    }

    fn sync_lane(&mut self, lane: usize) {
        for signal in 0..self.design.signals.len() {
            self.state[signal * self.batch_size + lane] =
                self.lanes[lane].read(SignalId(signal)).clone();
        }
    }

    fn sync_all(&mut self) {
        for lane in 0..self.batch_size {
            self.sync_lane(lane);
        }
    }

    fn failure(lane: usize, cycle: u64, error: RuntimeError) -> BatchFailure {
        BatchFailure {
            batch_idx: lane,
            cycle,
            error,
        }
    }

    /// Drive one input in one trajectory.
    pub fn drive(
        &mut self,
        lane: usize,
        signal: SignalId,
        value: Value,
    ) -> Result<(), RuntimeError> {
        self.check_lane(lane)?;
        self.lanes[lane].drive(signal, value)?;
        self.sync_lane(lane);
        Ok(())
    }

    /// Drive one named input in one trajectory.
    pub fn drive_named(
        &mut self,
        lane: usize,
        name: &str,
        value: Value,
    ) -> Result<(), RuntimeError> {
        self.check_lane(lane)?;
        self.lanes[lane].drive_named(name, value)?;
        self.sync_lane(lane);
        Ok(())
    }

    /// Apply one input value to every trajectory.
    pub fn broadcast(&mut self, signal: SignalId, value: Value) -> Result<(), RuntimeError> {
        for lane in 0..self.batch_size {
            self.lanes[lane].drive(signal, value.clone())?;
        }
        self.sync_all();
        Ok(())
    }

    /// Apply one named input value to every trajectory.
    pub fn broadcast_named(&mut self, name: &str, value: Value) -> Result<(), RuntimeError> {
        for lane in 0..self.batch_size {
            self.lanes[lane].drive_named(name, value.clone())?;
        }
        self.sync_all();
        Ok(())
    }

    /// Drive one input with one value per trajectory.
    pub fn drive_batch(&mut self, signal: SignalId, values: &[Value]) -> Result<(), RuntimeError> {
        if values.len() != self.batch_size {
            return Err(RuntimeError::Invalid(format!(
                "drive batch has {} values, expected {}",
                values.len(),
                self.batch_size
            )));
        }
        for (lane, value) in values.iter().enumerate() {
            self.lanes[lane].drive(signal, value.clone())?;
        }
        self.sync_all();
        Ok(())
    }

    /// Evaluate combinational logic for every trajectory and report the first
    /// failing lane in deterministic order.
    pub fn eval_comb(&mut self) -> Result<(), BatchFailure> {
        for lane in 0..self.batch_size {
            let cycle = self.lanes[lane].cycle();
            if let Err(error) = self.lanes[lane].eval_comb() {
                self.sync_lane(lane);
                return Err(Self::failure(lane, cycle, error));
            }
            self.sync_lane(lane);
        }
        Ok(())
    }

    /// Tick the named clock in every trajectory and report the first failure.
    pub fn tick(&mut self, clock: &str) -> Result<(), BatchFailure> {
        for lane in 0..self.batch_size {
            let cycle = self.lanes[lane].cycle();
            if let Err(error) = self.lanes[lane].tick(clock) {
                self.sync_lane(lane);
                return Err(Self::failure(lane, cycle, error));
            }
            self.sync_lane(lane);
        }
        Ok(())
    }

    /// Read one signal from one trajectory.
    pub fn read(&self, lane: usize, signal: SignalId) -> Option<&Value> {
        self.signal_state(signal, lane)
    }

    /// Read a named top-level signal from one trajectory.
    pub fn read_named(&self, lane: usize, name: &str) -> Result<&Value, RuntimeError> {
        self.check_lane(lane)?;
        let signal = self.lanes[lane].port(name)?;
        Ok(&self.state[signal.0 * self.batch_size + lane])
    }

    /// Copy all lanes of one signal into a caller-owned vector.
    pub fn read_batch(&self, signal: SignalId) -> Vec<Value> {
        (0..self.batch_size)
            .map(|lane| self.state[signal.0 * self.batch_size + lane].clone())
            .collect()
    }

    /// Read a memory word from one trajectory.
    pub fn memory(&self, lane: usize, memory: MemId, address: u32) -> Option<&Value> {
        self.lanes.get(lane)?.memory(memory, address)
    }
}
