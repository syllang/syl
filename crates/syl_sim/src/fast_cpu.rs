//! Word-plane execution for single-clock, synchronous, known two-state designs.
mod compile;
mod family;
mod table;
pub use family::FastFamilyProgram;
pub(crate) mod kernel;
use std::sync::{Mutex, OnceLock};

use crate::{RuntimeError, SignalId, SimDesign};
use kernel::FastOp;

fn mask(width: u32) -> u64 {
    u64::MAX >> (64 - width)
}

/// A compiled program can be shared by any number of independent batches.
pub struct FastProgram {
    comb_ends: Vec<usize>,
    pub(crate) comb: Vec<FastOp>,
    pub(crate) edge: Vec<FastOp>,
    pub(crate) slots: usize,
    pub(crate) clock: Option<usize>,
    pub(crate) signals: Vec<(usize, u32)>,
    inputs: Vec<bool>,
    /// A change in clock level can change a combinational assignment.
    pub clock_used_in_comb: bool,
}

impl FastProgram {
    pub(crate) fn storage_bytes(&self, batch: usize) -> Option<usize> {
        self.slots
            .checked_add(1)?
            .checked_mul(batch)?
            .checked_mul(size_of::<u64>())
            .filter(|&bytes| bytes <= isize::MAX as usize)
    }
    /// Compile the combinational DAG and a read-before-write rising-edge schedule.
    pub fn new(design: &SimDesign) -> Result<Self, RuntimeError> {
        compile::compile(design)
    }
}

/// A cache tile owns one contiguous slot-major plane and its preallocated scratch row.
pub struct FastTile {
    lanes: usize,
    // Exactly one of these owns the plane. A shared observer can settle it
    // once and borrow the result; the next mutable operation takes it back.
    working: Mutex<Buffers>,
    observation: OnceLock<Vec<u64>>,
    cycle: u64,
    settled: bool,
}
struct Buffers {
    plane: Vec<u64>,
    scratch: Vec<u64>,
}

impl FastTile {
    pub fn lanes(&self) -> usize {
        self.lanes
    }

    /// Access one 64-bit limb of an input. Mask unused high bits before evaluation.
    pub fn input_word(
        &mut self,
        program: &FastProgram,
        signal: SignalId,
        word: usize,
    ) -> &mut [u64] {
        self.adopt_observation();
        self.settled = false;
        assert!(program.inputs[signal.0]);
        let (base, width) = program.signals[signal.0];
        assert!(word < width.div_ceil(64) as usize);
        let stride = self.lanes();
        let start = (base + word) * stride;
        &mut self.working.get_mut().unwrap().plane[start..start + stride]
    }

    pub fn word(&self, program: &FastProgram, signal: SignalId, word: usize) -> &[u64] {
        let (base, width) = program.signals[signal.0];
        assert!(word < width.div_ceil(64) as usize);
        let stride = self.lanes();
        let start = (base + word) * stride;
        let plane = self.observation.get_or_init(|| {
            let mut buffers = self.working.lock().unwrap();
            let Buffers { plane, scratch } = &mut *buffers;
            if !self.settled {
                kernel::execute(&program.comb, plane, scratch, false);
            }
            std::mem::take(plane)
        });
        &plane[start..start + stride]
    }

    fn adopt_observation(&mut self) {
        if let Some(plane) = self.observation.take() {
            self.working.get_mut().unwrap().plane = plane;
            self.settled = true;
        }
    }

    pub fn eval_comb(&mut self, program: &FastProgram) {
        self.adopt_observation();
        self.settled = true;
        let Buffers { plane, scratch } = self.working.get_mut().unwrap();
        kernel::execute(&program.comb, plane, scratch, false);
    }

    /// Sample the rising edge, check every assertion on old state, then settle low.
    /// A failed tile returns its lowest failing lane and one-based edge number.
    pub fn tick(&mut self, program: &FastProgram) -> Result<(), (usize, u64)> {
        self.step(program, true)
            .map_or(Ok(()), |(lane, cycle, _)| Err((lane, cycle)))
    }

    pub(crate) fn step(
        &mut self,
        program: &FastProgram,
        stop: bool,
    ) -> Option<(usize, u64, usize)> {
        self.adopt_observation();
        let stride = self.lanes();
        if let Some(clk) = program.clock {
            self.working.get_mut().unwrap().plane[clk * stride..(clk + 1) * stride].fill(1);
        }
        if program.clock_used_in_comb || !self.settled {
            self.eval_comb(program);
        }
        let Buffers { plane, scratch } = self.working.get_mut().unwrap();
        let failure = kernel::execute(&program.edge, plane, scratch, stop)
            .map(|(lane, id)| (lane, self.cycle + 1, id));
        if stop && failure.is_some() {
            return failure;
        }
        if let Some(clk) = program.clock {
            self.working.get_mut().unwrap().plane[clk * stride..(clk + 1) * stride].fill(0);
        }
        self.settled = false;
        self.cycle += 1;
        failure
    }
}

/// Tiles have disjoint ownership. Worker creation is amortized over the entire run.
pub struct FastBatchCpuSimulator {
    tiles: Vec<FastTile>,
    tile_size: usize,
}

impl FastBatchCpuSimulator {
    pub fn new(program: &FastProgram, batch: usize, tile_size: usize) -> Self {
        assert!(batch > 0 && tile_size > 0);
        let tiles = (0..batch)
            .step_by(tile_size)
            .map(|start| {
                let lanes = tile_size.min(batch - start);
                FastTile {
                    lanes,
                    working: Mutex::new(Buffers {
                        plane: vec![0; program.slots * lanes],
                        scratch: vec![0; lanes],
                    }),
                    cycle: 0,
                    settled: false,
                    observation: OnceLock::new(),
                }
            })
            .collect();
        Self { tiles, tile_size }
    }

    /// Run a complete trajectory loop per tile. The callback receives its global lane offset.
    pub fn for_each_tile(&mut self, threads: usize, run: impl Fn(usize, &mut FastTile) + Sync) {
        assert!(threads > 0);
        let tile_size = self.tile_size;
        let group = self.tiles.len().div_ceil(threads);
        let visit = |first: usize, tiles: &mut [FastTile]| {
            for (i, tile) in tiles.iter_mut().enumerate() {
                run((first + i) * tile_size, tile);
            }
        };
        if threads == 1 || self.tiles.len() == 1 {
            visit(0, &mut self.tiles);
            return;
        }
        let ready = std::sync::Barrier::new(self.tiles.len().div_ceil(group));
        std::thread::scope(|scope| {
            for (i, tiles) in self.tiles.chunks_mut(group).enumerate() {
                let ready = &ready;
                scope.spawn(move || {
                    ready.wait();
                    visit(i * group, tiles);
                });
            }
        });
    }
}
