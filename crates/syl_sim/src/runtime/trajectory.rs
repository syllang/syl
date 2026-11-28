//! The Value batch owns slot-major signals. All event history and RAM are
//! trajectory-local. Temporarily lend one trajectory to the reference evaluator
//! so every public execution mode uses its exact clock and expression semantics.
use super::*;

#[derive(Clone, Default)]
pub(crate) struct TrajectoryState {
    pub(super) clock_sources: Vec<SignalId>,
    pub(super) last_clocks: Vec<Logic>,
    pub(super) last_resets: Vec<Logic>,
    pub(super) inout_drives: BTreeMap<SignalId, InoutNet>,
    pub(super) memories: Vec<Vec<Value>>,
    pub(super) cycle: u64,
}

impl TrajectoryState {
    pub(crate) fn new(design: &SimDesign, mode: StateMode) -> (Self, Vec<Value>) {
        let sim = Simulator::with_mode(design, mode);
        (sim.trajectory, sim.values)
    }

    pub(crate) fn cycle(&self) -> u64 {
        self.cycle
    }
    pub(crate) fn memory(&self, id: MemId, address: u32) -> Option<&Value> {
        self.memories.get(id.0)?.get(address as usize)
    }

    pub(crate) fn drive(
        &mut self,
        design: &SimDesign,
        mode: StateMode,
        id: SignalId,
        value: Value,
    ) -> Result<Value> {
        if !design
            .top_ports()
            .iter()
            .any(|p| p.signal == id && p.direction != HwDirection::Out)
        {
            return Err(RuntimeError::Invalid(format!("{id:?} is not a top input")));
        }
        let v = value.resized(design.signal(id).width);
        if let Some(driver) = self.inout_drives.get_mut(&id) {
            driver.pin.clone_from(&v);
        }
        Ok(if mode == StateMode::TwoState {
            v.into_two_state()
        } else {
            v
        })
    }

    pub(crate) fn execute(
        &mut self,
        design: &SimDesign,
        mode: StateMode,
        values: &mut Vec<Value>,
        operation: impl FnOnce(&mut Simulator<'_>) -> Result<()>,
    ) -> Result<()> {
        let mut sim = Simulator {
            design,
            mode,
            values: std::mem::take(values),
            trajectory: std::mem::take(self),
            models: BTreeMap::new(),
            assertions: AssertionMode::Stop,
            log_lines: Vec::new(),
            log_sink: None,
            stimulus_step: None,
            strict_unique: false,
            unique_violations: Vec::new(),
        };
        let result = operation(&mut sim);
        *self = sim.trajectory;
        *values = sim.values;
        result
    }
}
