//! Deterministic T3 protocol. Ports occupy consecutive 64-bit limbs in top
//! declaration order (including clock/reset). Cycles start at one, lanes at
//! zero. The checksum sums final output words times (2 * SoA index + 1),
//! modulo 2^64. Assertion detection does not interrupt this reduction.
use crate::backend::{LocalBackend, Program};
use crate::gpu_cuda::{CudaCache, KernelMode, RunnerOptions};
use crate::runtime::AssertionMode;
use crate::*;
use std::io::Write;
use std::sync::Mutex;

#[derive(Clone, Copy, Debug)]
pub struct BatchOptions<'a> {
    pub batch: usize,
    pub cycles: u64,
    pub seed: u64,
    pub threads: usize,
    pub cache: CudaCache<'a>,
    pub state_mode: StateMode,
    pub strict_unique: bool,
    pub fail_on_x: bool,
}

impl Default for BatchOptions<'_> {
    fn default() -> Self {
        Self {
            batch: 1,
            cycles: 1,
            seed: 0,
            threads: 1,
            cache: CudaCache::Default,
            state_mode: StateMode::TwoState,
            strict_unique: false,
            fail_on_x: false,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct FailureCoordinate {
    pub cycle: u64,
    pub batch: usize,
    pub assert_id: AssertId,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BatchResult {
    pub digest: u64,
    pub failure: Option<(FailureCoordinate, AssertionFailure)>,
    pub cache_hit: Option<bool>,
    pub compile_ms: f64,
    pub kernel_ms: f64,
}

/// SplitMix64 finalizer keyed by seed, global lane, one-based cycle, first
/// word index of the port, and little-endian limb. Seed zero preserves T3.
pub fn batch_stimulus_word(seed: u64, batch: usize, cycle: u64, port_word: u64, limb: u32) -> u64 {
    let mut z = seed
        .wrapping_add((batch as u64).rotate_left(32))
        .wrapping_add(cycle)
        .wrapping_add(
            0x9e3779b97f4a7c15u64
                .wrapping_mul(port_word.wrapping_add(u64::from(limb)).wrapping_add(1)),
        );
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}

struct Input {
    id: SignalId,
    width: u32,
    base: u64,
    reset: Option<HwResetPolarity>,
}
impl Input {
    fn word(&self, seed: u64, lane: usize, cycle: u64, limb: u32) -> u64 {
        if let Some(p) = self.reset {
            return u64::from((cycle == 1) == (p == HwResetPolarity::ActiveHigh));
        }
        batch_stimulus_word(seed, lane, cycle, self.base, limb)
            & (u64::MAX >> (64 - (self.width - 64 * limb).min(64)))
    }
}

struct Ports {
    inputs: Vec<Input>,
    outputs: Vec<(SignalId, u32)>,
    rising: Vec<ClockEvent>,
    falling: Vec<ClockEvent>,
}
impl Ports {
    fn new(d: &SimDesign) -> Self {
        let clocks = runtime::clock_sources(d);
        let mut ports = Self {
            inputs: vec![],
            outputs: vec![],
            rising: vec![],
            falling: vec![],
        };
        let mut base = 0;
        for p in d.top_ports() {
            let width = d.signal(p.signal).width;
            if p.direction != HwDirection::Out {
                if let Some(i) = clocks.iter().position(|&clk| clk == p.signal) {
                    let id = DomainId(i);
                    ports.rising.push(ClockEvent::Rising(id));
                    ports.falling.push(ClockEvent::Falling(id));
                } else {
                    ports.inputs.push(Input {
                        id: p.signal,
                        width,
                        base,
                        reset: d
                            .resets
                            .iter()
                            .find(|r| r.signal == p.signal)
                            .map(|r| r.polarity),
                    });
                }
            }
            if p.direction != HwDirection::In {
                ports
                    .outputs
                    .extend((0..width.div_ceil(64)).map(|limb| (p.signal, limb)));
            }
            base += u64::from(width.div_ceil(64));
        }
        ports
    }
    fn drive(
        &self,
        sim: &mut Simulator,
        seed: u64,
        lane: usize,
        cycle: u64,
    ) -> Result<(), RuntimeError> {
        for p in &self.inputs {
            let mut value = Value::from_u64(p.width, p.word(seed, lane, cycle, 0));
            for limb in 1..p.width.div_ceil(64) {
                value.insert(
                    limb * 64,
                    &Value::from_u64(
                        (p.width - limb * 64).min(64),
                        p.word(seed, lane, cycle, limb),
                    ),
                );
            }
            sim.drive(p.id, value)?;
        }
        Ok(())
    }
}

fn weighted(word: u64, index: usize, batch: usize, lane: usize) -> u64 {
    let index = (index as u64)
        .wrapping_mul(batch as u64)
        .wrapping_add(lane as u64);
    word.wrapping_mul(index.wrapping_mul(2).wrapping_add(1))
}

#[derive(Default)]
struct Reduction {
    digest: u64,
    failure: Option<FailureCoordinate>,
}
impl Reduction {
    fn record(&mut self, c: FailureCoordinate) {
        if self.failure.is_none_or(|f| c < f) {
            self.failure = Some(c);
        }
    }
}

fn simd(
    d: &SimDesign,
    o: BatchOptions,
    ports: &Ports,
    program: &FastProgram,
) -> Result<Reduction, RuntimeError> {
    let workers = o
        .threads
        .min(std::thread::available_parallelism().map_or(1, usize::from));
    if program.storage_bytes(o.batch).is_none() {
        return Err(RuntimeError::Invalid(
            "batch state exceeds host address space".into(),
        ));
    }
    let size = o.batch.div_ceil(workers).clamp(1, 128);
    let mut sim = FastBatchCpuSimulator::new(program, o.batch, size);
    let summary = Mutex::new(Reduction::default());
    sim.for_each_tile(workers, |start, tile| {
        let mut result = Reduction::default();
        for cycle in 1..=o.cycles {
            for p in &ports.inputs {
                for limb in 0..p.width.div_ceil(64) {
                    for (l, word) in tile
                        .input_word(program, p.id, limb as usize)
                        .iter_mut()
                        .enumerate()
                    {
                        *word = p.word(o.seed, start + l, cycle, limb);
                    }
                }
            }
            if let Some((lane, cycle, id)) = tile.step(program, false) {
                result.record(FailureCoordinate {
                    cycle,
                    batch: start + lane,
                    assert_id: AssertId(id),
                });
            }
        }
        if o.cycles == 0 {
            tile.eval_comb(program);
        }
        for (i, &(id, limb)) in ports.outputs.iter().enumerate() {
            for (l, &word) in tile.word(program, id, limb as usize).iter().enumerate() {
                let width = (d.signal(id).width - limb * 64).min(64);
                result.digest = result.digest.wrapping_add(weighted(
                    word & (u64::MAX >> (64 - width)),
                    i,
                    o.batch,
                    start + l,
                ));
            }
        }
        let mut total = summary.lock().unwrap();
        total.digest = total.digest.wrapping_add(result.digest);
        if let Some(f) = result.failure {
            total.record(f);
        }
    });
    Ok(summary.into_inner().unwrap())
}

fn initial_sim(d: &SimDesign, mode: StateMode) -> Result<Simulator<'_>, RuntimeError> {
    let mut sim = Simulator::with_mode(d, mode);
    for p in d
        .top_ports()
        .iter()
        .filter(|p| p.direction != HwDirection::Out)
    {
        sim.drive(p.signal, Value::zero(d.signal(p.signal).width))?;
    }
    Ok(sim)
}

fn trajectory<'a>(
    mut sim: Simulator<'a>,
    o: BatchOptions,
    ports: &Ports,
    lane: usize,
    mut sample: impl FnMut(&Simulator) -> Result<(), RuntimeError>,
) -> Result<Simulator<'a>, RuntimeError> {
    let d = sim.design();
    sim.strict_unique = o.strict_unique;
    // A zero-edge run still settles and observes the initial inputs at step zero.
    let first_step = u64::from(o.cycles != 0);
    for cycle in first_step..=o.cycles {
        sim.stimulus_step = Some(cycle);
        if cycle != 0 {
            ports.drive(&mut sim, o.seed, lane, cycle)?;
        }
        let settled = sim.eval_comb();
        sample(&sim)?;
        settled?;
        if cycle != 0 && !ports.rising.is_empty() {
            for events in [&ports.rising, &ports.falling] {
                let edge = sim.step_clocks(events);
                sample(&sim)?;
                edge?;
            }
        }
        if o.fail_on_x
            && let Some(p) = d
                .top_ports()
                .iter()
                .find(|p| p.direction != HwDirection::In && !sim.read(p.signal).is_known())
        {
            return Err(RuntimeError::Invalid(format!(
                "batch {lane}, cycle {cycle}: unknown output {}={}",
                p.name,
                sim.read(p.signal)
            )));
        }
    }
    Ok(sim)
}

fn reference(d: &SimDesign, o: BatchOptions, ports: &Ports) -> Result<Reduction, RuntimeError> {
    let mut result = Reduction::default();
    for lane in 0..o.batch {
        let mut sim = initial_sim(d, o.state_mode)?;
        sim.assertions = AssertionMode::Record(None);
        let sim = trajectory(sim, o, ports, lane, |_| Ok(()))?;
        if let AssertionMode::Record(Some((cycle, assert_id))) = sim.assertions {
            result.record(FailureCoordinate {
                cycle,
                batch: lane,
                assert_id,
            });
        }
        for (i, &(id, limb)) in ports.outputs.iter().enumerate() {
            let value = sim.read(id);
            let word = value
                .slice(limb * 64, (value.width() - limb * 64).min(64))
                .into_two_state()
                .as_u64()
                .unwrap();
            result.digest = result.digest.wrapping_add(weighted(word, i, o.batch, lane));
        }
    }
    Ok(result)
}

/// Replay a captured edge, checking both cycle and assertion identity. The
/// waveform includes the settled initial pin values and the failing transition.
pub fn replay_failure(
    d: &SimDesign,
    o: BatchOptions,
    expected: FailureCoordinate,
    writer: Option<&mut dyn Write>,
) -> Result<AssertionFailure, RuntimeError> {
    let io_error = |e: std::io::Error| RuntimeError::Invalid(e.to_string());
    let mut wave = writer
        .map(|w| VcdWriter::new(w, d))
        .transpose()
        .map_err(io_error)?;
    let mut time = 0;
    if let Some(w) = &mut wave
        && expected.cycle.min(o.cycles) > 0
    {
        // Observing the undriven baseline must not commit a generated edge or
        // evaluate strict-unique checks outside the requested stimulus steps.
        let mut initial = initial_sim(d, o.state_mode)?;
        initial.settle_initial_pins()?;
        w.sample(time, &initial).map_err(io_error)?;
        time += 1;
    }
    let mut sim = initial_sim(d, o.state_mode)?;
    sim.assertions = AssertionMode::Replay {
        step: expected.cycle,
        id: expected.assert_id,
    };
    let result = trajectory(
        sim,
        BatchOptions {
            cycles: expected.cycle.min(o.cycles),
            ..o
        },
        &Ports::new(d),
        expected.batch,
        |sim| {
            if let Some(w) = &mut wave {
                w.sample(time, sim).map_err(io_error)?;
                time += 1;
            }
            Ok(())
        },
    );
    if let Some(w) = wave {
        w.finish().map_err(io_error)?;
    }
    match result {
        Err(RuntimeError::Assertion(f))
            if f.cycle == expected.cycle && f.assert_id == expected.assert_id =>
        {
            Ok(f)
        }
        Err(e) => Err(RuntimeError::Invalid(format!(
            "replay mismatch at {expected:?}: {e}"
        ))),
        Ok(_) => Err(RuntimeError::Invalid(format!(
            "replay missed assertion at {expected:?}"
        ))),
    }
}

/// Execute one batch and verify the first failure against the reference simulator.
pub fn run_batch(
    d: &SimDesign,
    backend: ResolvedBackend,
    o: BatchOptions,
) -> Result<BatchResult, RuntimeError> {
    let (choice, gpu_mode) = match backend {
        ResolvedBackend::CpuRef => (SimBackendChoice::CpuRef, GpuKernelMode::Auto),
        ResolvedBackend::CpuSimd => (SimBackendChoice::CpuSimd, GpuKernelMode::Auto),
        ResolvedBackend::GpuCuda(KernelMode::BatchPerThread) => {
            (SimBackendChoice::GpuCuda, GpuKernelMode::Batch)
        }
        ResolvedBackend::GpuCuda(KernelMode::InstanceFamilyParallel) => {
            (SimBackendChoice::GpuCuda, GpuKernelMode::Family)
        }
    };
    let r = BackendRequest {
        choice,
        gpu_mode,
        batch: o.batch,
        cycles: o.cycles,
        state_mode: o.state_mode,
        strict_unique: o.strict_unique,
        fail_on_x: o.fail_on_x,
        ..Default::default()
    };
    if o.threads == 0 {
        return Err(RuntimeError::Invalid("threads must be positive".into()));
    }
    select_local_backend(d, &r, o.cache)
        .map_err(RuntimeError::Invalid)?
        .run(o.seed, o.threads)
}

impl LocalBackend<'_> {
    /// Execute a deterministic batch with this plan's dimensions and semantics.
    pub fn run(&self, seed: u64, threads: usize) -> Result<BatchResult, RuntimeError> {
        self.run_with_replay(seed, threads, || Ok(None::<std::io::Sink>))
    }

    /// Open the optional VCD destination only after a failure is captured, and
    /// record the same reference execution that verifies its coordinates.
    pub fn run_with_replay<W: Write>(
        &self,
        seed: u64,
        threads: usize,
        open: impl FnOnce() -> Result<Option<W>, RuntimeError>,
    ) -> Result<BatchResult, RuntimeError> {
        if threads == 0 {
            return Err(RuntimeError::Invalid("threads must be positive".into()));
        }
        if self.request.stimulus || self.request.direct_vcd {
            return Err(RuntimeError::Invalid(
                "explicit trace stimuli require the Simulator trace API".into(),
            ));
        }
        let d = self.design;
        let r = &self.request;
        let o = BatchOptions {
            batch: r.batch,
            cycles: r.cycles,
            seed,
            threads,
            state_mode: r.state_mode,
            strict_unique: r.strict_unique,
            fail_on_x: r.fail_on_x,
            ..Default::default()
        };
        let mut result = BatchResult::default();
        let summary = match &self.program {
            Program::Reference => reference(d, o, &Ports::new(d))?,
            Program::Simd(program) => simd(d, o, &Ports::new(d), program)?,
            Program::Cuda(program) => {
                let options = RunnerOptions {
                    batch: o.batch,
                    cycles: o.cycles,
                    ..Default::default()
                };
                let gpu = self
                    .toolchain
                    .as_ref()
                    .expect("validated CUDA device")
                    .run(&program.source, options, seed)
                    .map_err(RuntimeError::Invalid)?;
                result.cache_hit = Some(gpu.cache_hit);
                result.compile_ms = gpu.compile_ms;
                result.kernel_ms = gpu.elapsed_ms;
                Reduction {
                    digest: gpu.digest,
                    failure: gpu.fail_batch.zip(gpu.fail_cycle).zip(gpu.fail_assert).map(
                        |((batch, cycle), id)| FailureCoordinate {
                            batch,
                            cycle,
                            assert_id: AssertId(id),
                        },
                    ),
                }
            }
        };
        result.digest = summary.digest;
        result.failure = summary
            .failure
            .map(|c| {
                let mut writer = open()?;
                replay_failure(d, o, c, writer.as_mut().map(|w| w as &mut dyn Write))
                    .map(|f| (c, f))
            })
            .transpose()?;
        Ok(result)
    }
}
