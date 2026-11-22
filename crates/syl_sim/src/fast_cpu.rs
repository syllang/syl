//! Word-plane execution for single-clock, synchronous, known two-state designs.
mod compile;
mod kernel;

use crate::{RuntimeError, SignalId, SimDesign};
use kernel::FastOp;

fn mask(width: u32) -> u64 {
    u64::MAX >> (64 - width)
}

/// A compiled program can be shared by any number of independent batches.
pub struct FastProgram {
    comb: Vec<FastOp>,
    edge: Vec<FastOp>,
    slots: usize,
    clock: Option<usize>,
    signals: Vec<(usize, u32)>,
    inputs: Vec<bool>,
}

impl FastProgram {
    /// Compile the combinational DAG and a read-before-write rising-edge schedule.
    pub fn new(design: &SimDesign) -> Result<Self, RuntimeError> {
        compile::compile(design)
    }
}

/// A cache tile owns one contiguous slot-major plane and its preallocated scratch row.
pub struct FastTile {
    plane: Vec<u64>,
    scratch: Vec<u64>,
    cycle: u64,
}

impl FastTile {
    pub fn lanes(&self) -> usize {
        self.scratch.len()
    }

    /// Access one 64-bit limb of an input. Mask unused high bits before evaluation.
    pub fn input_word(
        &mut self,
        program: &FastProgram,
        signal: SignalId,
        word: usize,
    ) -> &mut [u64] {
        assert!(program.inputs[signal.0]);
        let (base, width) = program.signals[signal.0];
        assert!(word < width.div_ceil(64) as usize);
        let stride = self.lanes();
        let start = (base + word) * stride;
        &mut self.plane[start..start + stride]
    }

    pub fn word(&self, program: &FastProgram, signal: SignalId, word: usize) -> &[u64] {
        let (base, width) = program.signals[signal.0];
        assert!(word < width.div_ceil(64) as usize);
        let stride = self.lanes();
        let start = (base + word) * stride;
        &self.plane[start..start + stride]
    }

    pub fn eval_comb(&mut self, program: &FastProgram) {
        kernel::execute(&program.comb, &mut self.plane, &mut self.scratch);
    }

    /// Sample the rising edge, check every assertion on old state, then settle low.
    /// A failed tile returns its lowest failing lane and one-based edge number.
    pub fn tick(&mut self, program: &FastProgram) -> Result<(), (usize, u64)> {
        let stride = self.lanes();
        if let Some(clk) = program.clock {
            self.plane[clk * stride..(clk + 1) * stride].fill(1);
        }
        self.eval_comb(program);
        if let Some(lane) = kernel::execute(&program.edge, &mut self.plane, &mut self.scratch) {
            return Err((lane, self.cycle + 1));
        }
        if let Some(clk) = program.clock {
            self.plane[clk * stride..(clk + 1) * stride].fill(0);
        }
        self.eval_comb(program);
        self.cycle += 1;
        Ok(())
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
                    plane: vec![0; program.slots * lanes],
                    scratch: vec![0; lanes],
                    cycle: 0,
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
