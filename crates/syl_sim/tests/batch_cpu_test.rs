#[path = "../examples/bench_cpu_2d/workload.rs"]
mod workload;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;
use syl_sim::{
    FastBatchCpuSimulator, FastProgram, FastTile, HwDirection, SimDesign, Simulator, StateMode,
};
use workload::{Port, fixture, ports, scalar_value, stimulus};

// Count on the thread executing the tile, excluding setup and the allocating T2 oracle.
struct CountingAllocator;
thread_local! {
    static ALLOCATIONS: Cell<Option<usize>> = const { Cell::new(None) };
}

#[global_allocator]
static ALLOCATOR: CountingAllocator = CountingAllocator;

// SAFETY: System performs every allocation and deallocation with the caller's layout.
// The const thread-local counter has no destructor and does not allocate.
unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATIONS.with(|count| count.set(count.get().map(|n| n + 1)));
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }
}

fn drive_tile(
    tile: &mut FastTile,
    program: &FastProgram,
    inputs: &[Port],
    first: usize,
    cycle: u64,
) {
    for &port in inputs {
        for word in 0..port.width.div_ceil(64) {
            let row = tile.input_word(program, port.id, word as usize);
            for (offset, dst) in row.iter_mut().enumerate() {
                *dst = stimulus(port, word, first + offset, cycle);
            }
        }
    }
}

#[test]
fn every_output_word_matches_reference_before_and_after_each_edge() {
    for case in [
        "RoundRobinArbiter4",
        "SyncFifo-D8",
        "SyncFifo-D64",
        "SimpleDualPortRam",
        "LaneArray-N1",
        "LaneArray-N4",
        "LaneArray-N16",
    ] {
        let (ir, top, params) = fixture(case);
        let design = SimDesign::lower(&ir, top, &params).unwrap();
        let program = FastProgram::new(&design).unwrap();
        let inputs = ports(&design, HwDirection::In);
        let outputs = ports(&design, HwDirection::Out);
        for threads in [1, 4] {
            let mut batch = FastBatchCpuSimulator::new(&program, 17, 5);
            batch.for_each_tile(threads, |first, tile| {
                let mut references: Vec<_> = (0..tile.lanes())
                    .map(|_| Simulator::with_mode(&design, StateMode::TwoState))
                    .collect();
                for cycle in 1..=32 {
                    ALLOCATIONS.set(Some(0));
                    drive_tile(tile, &program, &inputs, first, cycle);
                    tile.eval_comb(&program);
                    assert_eq!(ALLOCATIONS.replace(None), Some(0));

                    for (offset, reference) in references.iter_mut().enumerate() {
                        for &port in &inputs {
                            reference.drive(port.id, scalar_value(port, first + offset, cycle)).unwrap();
                        }
                        reference.eval_comb().unwrap();
                    }

                    for after_edge in [false, true] {
                        if after_edge {
                            ALLOCATIONS.set(Some(0));
                            tile.tick(&program).unwrap();
                            assert_eq!(ALLOCATIONS.replace(None), Some(0));
                        }
                        for (offset, reference) in references.iter_mut().enumerate() {
                            if after_edge {
                                reference.tick("clk").unwrap();
                            }
                            for port in &outputs {
                                for word in 0..port.width.div_ceil(64) {
                                    let bits = (port.width - word * 64).min(64);
                                    let expected = reference.read(port.id).slice(word * 64, bits);
                                    let actual = tile.word(&program, port.id, word as usize)[offset];
                                    assert_eq!(Some(actual), expected.as_u64(),
                                        "{case} lane {} cycle {cycle} after_edge={after_edge} port {:?} word {word}",
                                        first + offset, port.id);
                                }
                            }
                        }
                    }
                }
            });
        }
    }
}
