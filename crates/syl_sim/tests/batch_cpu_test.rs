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

fn lower_cell(top: &str, source: &str) -> SimDesign {
    let ast = syl_syntax::SourceParser::new(source).parse_file().unwrap();
    let file = syl_sema::SemanticSourceFile::new(vec!["fixture".into()], &ast);
    let hir = syl_sema::SemanticSession::new_sources(vec![file])
        .resolve_hir()
        .unwrap();
    let ir = syl_elab::HardwareCompiler::new().output_for_tir(&hir.check_tir().unwrap());
    SimDesign::lower(&ir, top, &Default::default()).unwrap()
}

#[test]
fn falling_edges_and_direct_clock_drives_settle_like_t2() {
    use syl_sim::{BatchCpuSimulator, ClockEvent, Value};
    let design = lower_cell(
        "Top",
        "cell Top<D: Domain>(clk:in Clock<D>,rst:in Reset<D>,i:in Bit)->y:Bit {
        reg q:Bit reset(rst,0) next q:=i y:=q }",
    );
    let mut batch = BatchCpuSimulator::new(&design, 2);
    let mut reference = Simulator::with_mode(&design, StateMode::TwoState);
    let one = Value::from_u64(1, 1);
    reference.drive_named("i", one.clone()).unwrap();
    batch.broadcast_named("i", one.clone()).unwrap();
    let falling = [ClockEvent::Falling(reference.clock("clk").unwrap())];
    batch.step_clocks(&falling).unwrap();
    reference.step_clocks(&falling).unwrap();
    assert_eq!(batch.cycle(), 0);
    assert_eq!(batch.cycle(), reference.cycle());
    assert_eq!(
        batch.read_named(1, "y").unwrap(),
        reference.read_named("y").unwrap()
    );
    assert_eq!(reference.read_named("y").unwrap(), &Value::zero(1));
    batch.broadcast_named("clk", one.clone()).unwrap();
    reference.drive_named("clk", one.clone()).unwrap();
    batch.eval_comb().unwrap();
    reference.eval_comb().unwrap();
    assert_eq!(batch.cycle(), 1);
    assert_eq!(batch.cycle(), reference.cycle());
    assert_eq!(
        batch.read_named(1, "y").unwrap(),
        reference.read_named("y").unwrap()
    );
    assert_eq!(reference.read_named("y").unwrap(), &one);
}

#[test]
fn simultaneous_domains_sample_pre_edge_state_in_both_orders() {
    use syl_sim::{BatchCpuSimulator, ClockEvent, Value};
    let design = lower_cell(
        "PulseSynchronizer",
        include_str!("../../../examples/std/cdc.syl"),
    );
    for reverse in [false, true] {
        let mut reference = Simulator::with_mode(&design, StateMode::TwoState);
        let mut batch = BatchCpuSimulator::new(&design, 2);
        let domains = ["src_clk", "dst_clk"].map(|p| reference.clock(p).unwrap());
        for cycle in 1..=5 {
            let pulse = Value::from_u64(1, u64::from(cycle == 1));
            batch.broadcast_named("src_pulse", pulse.clone()).unwrap();
            reference.drive_named("src_pulse", pulse).unwrap();
            for edge in [ClockEvent::Rising, ClockEvent::Falling] {
                let mut events = domains.map(edge);
                if reverse {
                    events.reverse();
                }
                reference.step_clocks(&events).unwrap();
                batch.step_clocks(&events).unwrap();
                assert_eq!(batch.cycle(), cycle);
                assert_eq!(batch.cycle(), reference.cycle());
                for reg in &design.regs {
                    assert_eq!(
                        batch.read(1, reg.signal).unwrap(),
                        reference.read(reg.signal)
                    );
                }
                assert_eq!(
                    batch.read_named(1, "dst_pulse").unwrap(),
                    reference.read_named("dst_pulse").unwrap()
                );
            }
        }
    }
}

#[test]
fn all_public_clock_entry_points_report_the_failing_edge() {
    use syl_sim::{BatchCpuSimulator, ClockEvent, RuntimeError, Value};
    let (ir, top, params) = fixture("Fault");
    let d = SimDesign::lower(&ir, top, &params).unwrap();
    let reference = Simulator::with_mode(&d, StateMode::TwoState);
    let inject = reference.port("inject").unwrap();
    let p = FastProgram::new(&d).unwrap();
    let mut simd = FastBatchCpuSimulator::new(&p, 3, 3);
    simd.for_each_tile(1, |_, tile| {
        tile.input_word(&p, inject, 0)[1] = 1;
        assert_eq!(tile.tick(&p), Err((1, 1)));
    });
    for entry in ["tick", "events", "drive"] {
        let mut batch = BatchCpuSimulator::new(&d, 3);
        batch.drive(1, inject, Value::from_u64(1, 1)).unwrap();
        let failure = match entry {
            "tick" => batch.tick("clk"),
            "events" => batch.step_clocks(&[ClockEvent::Rising(reference.clock("clk").unwrap())]),
            _ => {
                batch.broadcast_named("clk", Value::from_u64(1, 1)).unwrap();
                batch.eval_comb()
            }
        }
        .unwrap_err();
        let RuntimeError::Assertion(inner) = failure.error else {
            panic!("expected assertion")
        };
        assert_eq!((failure.batch_idx, failure.cycle), (1, 1));
        assert_eq!(failure.cycle, inner.cycle);
    }
}

#[test]
fn four_state_batch_preserves_initial_storage_and_high_impedance() {
    use syl_sim::{BatchCpuSimulator, MemId, Value};
    let (ir, top, params) = fixture("SimpleDualPortRam");
    let d = SimDesign::lower(&ir, top, &params).unwrap();
    let batch = BatchCpuSimulator::with_mode(&d, 3, StateMode::FourState);
    let reference = Simulator::new(&d);
    for lane in 0..3 {
        assert_eq!(
            batch.read_named(lane, "rdata").unwrap(),
            reference.read_named("rdata").unwrap()
        );
        assert!(batch.read_named(lane, "rdata").unwrap().has_x());
        for address in 0..d.memories[0].depth {
            assert_eq!(
                batch.memory(lane, MemId(0), address),
                reference.memory(MemId(0), address)
            );
            assert!(batch.memory(lane, MemId(0), address).unwrap().has_x());
        }
    }
    let d = lower_cell("Top", "cell Top(p:inout Bit) {p:=z()}");
    let mut batch = BatchCpuSimulator::with_mode(&d, 2, StateMode::FourState);
    let mut reference = Simulator::new(&d);
    for pin in [Value::z(1), Value::from_u64(1, 1), Value::z(1)] {
        batch.drive_named(1, "p", pin.clone()).unwrap();
        reference.drive_named("p", pin.clone()).unwrap();
        batch.eval_comb().unwrap();
        reference.eval_comb().unwrap();
        assert_eq!(
            batch.read_named(1, "p").unwrap(),
            reference.read_named("p").unwrap()
        );
        assert_eq!(reference.read_named("p").unwrap(), &pin);
    }
}

#[test]
fn signed_operations_and_sign_filled_right_shift_match_t2() {
    use syl_sim::{HwBinaryOp as Op, HwUnaryOp, Selector, SimExpr as E};
    let source = "cell Top(i:in Bit)->q:UInt<8> {q:=select priority {i => (-3)/2, default=>0,}}";
    check_arithmetic(&lower_cell("Top", source), 255);
    let binary = |op, left, right| E::Binary {
        op,
        left: Box::new(left),
        right: Box::new(right),
    };
    let neg = |n| E::Unary {
        op: HwUnaryOp::Neg,
        expr: Box::new(E::Int(n)),
    };
    let quotient = binary(Op::Div, neg(3), E::Int(2));
    // The concrete IR has Shl and packed projections. An arithmetic right shift
    // by one repeats the sign bit and takes bits 7..1 of the two's-complement word.
    let slice = |low, high| E::Project {
        value: Box::new(neg(3)),
        selector: Selector::Slice { low, high },
    };
    let shift = E::Concat(vec![slice(7, 7), slice(1, 7)]);
    for (expr, expected) in [
        (quotient.clone(), 255),
        (binary(Op::Rem, neg(3), E::Int(2)), 255),
        (binary(Op::Div, neg(3), E::Int(0)), 0),
        (binary(Op::Lt, neg(3), E::Int(2)), 1),
        (binary(Op::GtEq, neg(3), neg(2)), 0),
        (binary(Op::Gt, neg(2), neg(3)), 1),
        (binary(Op::LtEq, neg(3), neg(3)), 1),
        (binary(Op::Add, quotient, E::Bool(false)), 254), // unsigned enclosing context
        (shift, 254),
    ] {
        let mut d = lower_cell("Top", "cell Top(i:in Bit)->q:UInt<8> {q:=i}");
        d.comb[0].value = expr;
        check_arithmetic(&d, expected);
    }
    let min = binary(Op::Shl, E::Int(1), E::Int(63));
    let mut wide = lower_cell("Top", "cell Top(i:in Bit)->q:UInt<64> {q:=i}");
    for (op, expected) in [(Op::Div, 1u64 << 63), (Op::Rem, 0)] {
        wide.comb[0].value = binary(op, min.clone(), neg(1));
        check_arithmetic(&wide, expected);
    }
}

fn check_arithmetic(design: &SimDesign, expected: u64) {
    use syl_sim::{BatchCpuSimulator, Value};
    let mut reference = Simulator::with_mode(design, StateMode::TwoState);
    let i = reference.port("i").unwrap();
    let q = reference.port("q").unwrap();
    reference.drive(i, Value::from_u64(1, 1)).unwrap();
    reference.eval_comb().unwrap();
    assert_eq!(reference.read(q).as_u64(), Some(expected));
    let mut batch = BatchCpuSimulator::new(design, 3);
    batch.broadcast(i, Value::from_u64(1, 1)).unwrap();
    batch.eval_comb().unwrap();
    assert!(batch.read_batch(q).iter().all(|v| v == reference.read(q)));
    let p = FastProgram::new(design).unwrap();
    let mut fast = FastBatchCpuSimulator::new(&p, 3, 2);
    fast.for_each_tile(2, |_, tile| {
        tile.input_word(&p, i, 0).fill(1);
        tile.eval_comb(&p);
        assert!(tile.word(&p, q, 0).iter().all(|&n| n == expected));
    });
}

// Compare every prefix, including the initialized output, against the same T3 input stream.
fn check_trace(
    case: &str,
    d: &SimDesign,
    cycles: u64,
    family: Option<&syl_sim::FastFamilyProgram>,
) {
    use syl_sim::{Value, batch_stimulus_word};
    let fast = FastProgram::new(d).unwrap();
    let mut sim = FastBatchCpuSimulator::new(&fast, 1, 1);
    sim.for_each_tile(1, |_, tile| {
        let mut reference = Simulator::with_mode(d, StateMode::TwoState);
        for cycle in 0..=cycles {
            let mut base = 0;
            for p in d.top_ports() {
                let w = d.signal(p.signal).width;
                if p.direction == HwDirection::In && p.name != "clk" {
                    let mut parts = Vec::new();
                    for j in (0..w.div_ceil(64)).rev() {
                        let bits = (w - j * 64).min(64);
                        let v = if cycle == 0 {
                            0
                        } else if p.name == "rst" {
                            u64::from(cycle == 1)
                        } else {
                            batch_stimulus_word(0, 0, cycle, base, j) & (u64::MAX >> (64 - bits))
                        };
                        parts.push(Value::from_u64(bits, v));
                        tile.input_word(&fast, p.signal, j as usize)[0] = v;
                    }
                    reference
                        .drive(p.signal, Value::concat(&parts).unwrap())
                        .unwrap();
                }
                base += u64::from(w.div_ceil(64));
            }
            if cycle == 0 {
                reference.eval_comb().unwrap();
            } else {
                reference.tick("clk").unwrap();
                tile.tick(&fast).unwrap();
            }
            let mut expected = Vec::new();
            for p in d
                .top_ports()
                .iter()
                .filter(|p| p.direction == HwDirection::Out)
            {
                let v = reference.read(p.signal);
                for j in 0..v.width().div_ceil(64) {
                    let word = v
                        .slice(j * 64, (v.width() - j * 64).min(64))
                        .as_u64()
                        .unwrap();
                    assert_eq!(
                        tile.word(&fast, p.signal, j as usize),
                        [word],
                        "{case} {} {j} cycle {cycle}",
                        p.name
                    );
                    expected.push(word);
                }
            }
            let oracle = syl_sim::SingleTraceResult {
                outputs: expected,
                failure: None,
            };
            if let Some(f) = family {
                assert_eq!(
                    f.run_trajectory(0, cycle),
                    oracle,
                    "{case} family cycle {cycle}"
                );
            }
        }
    });
}

#[test]
fn four_member_wide_subtraction_and_ordering_match_each_reference_limb() {
    use syl_sim::{HwBinaryOp as Op, HwUnaryOp, Selector, SimExpr as E};
    for width in [65, 73, 129] {
        // Ordered comparison of hardware values is available in concrete IR.
        // Start with the same signal dependencies to retain the compiled schedule.
        let source = format!("cell Alu<D:Domain>(clk:in Clock<D>,a:in UInt<{width}>,b:in UInt<{width}>,delta:out UInt<{width}>,flags:out [12] Bit){{
            // At cycle zero, 0 - 1 propagates a borrow through all limbs.
            signal rhs:UInt<{width}> := select priority {{ b eq 0 => 1, default => b, }}
            delta := a-rhs
            {}
        }}
        cell Top<D:Domain>(clk:in Clock<D>,a:in [4] UInt<{width}>,b:in [4] UInt<{width}>,delta:out [4] UInt<{width}>,flags:out [4] [12] Bit){{
            let lanes=for i in 0..4 {{place Alu<D>(clk:clk,a:a[i],b:b[i],delta:delta[i],flags:flags[i])}}
        }}", (0..12).map(|i|format!("flags[{i}] := a eq b")).collect::<Vec<_>>().join("\n"));
        let mut d = lower_cell("Top", &source);
        let binary = |op, x: E, y: E| E::Binary {
            op,
            left: Box::new(x),
            right: Box::new(y),
        };
        // Flipping the sign bit maps packed two's-complement order to unsigned order.
        let flip = |v: E| {
            let slice = |high, low| E::Project {
                value: Box::new(v.clone()),
                selector: Selector::Slice { high, low },
            };
            E::Concat(vec![
                E::Unary {
                    op: HwUnaryOp::Not,
                    expr: Box::new(slice(width - 1, width - 1)),
                },
                slice(width - 2, 0),
            ])
        };
        // Both mux arms are signed literals. Their natural comparison width is 65.
        let signed = |cond| E::Mux {
            cond: Box::new(cond),
            then_value: Box::new(E::Unary {
                op: HwUnaryOp::Neg,
                expr: Box::new(E::Int(1 << 63)),
            }),
            else_value: Box::new(E::Int(1 << 63)),
        };
        let mut predicates = 0;
        for assignment in &mut d.comb {
            let E::Binary {
                op: Op::Eq,
                left,
                right,
            } = &assignment.value
            else {
                continue;
            };
            let i = predicates % 12;
            let op = [Op::Lt, Op::LtEq, Op::Gt, Op::GtEq][i % 4];
            let (a, b) = (*left.clone(), *right.clone());
            assignment.value = match i {
                0..4 => binary(op, a, b),
                4..8 => binary(op, flip(a), flip(b)),
                _ => binary(
                    op,
                    signed(binary(Op::Lt, a.clone(), b.clone())),
                    signed(binary(Op::Gt, a, b)),
                ),
            };
            predicates += 1;
        }
        assert_eq!(predicates, 4 * 12);
        let family = syl_sim::FastFamilyProgram::new(&d).unwrap();
        check_trace(&format!("wide {width}"), &d, 16, Some(&family));
    }
}

#[test]
fn family_matches_each_settled_output_limb_on_lane_arrays() {
    for case in ["LaneArray-N4", "LaneArray-N16", "LaneArray-N64"] {
        let (ir, top, params) = fixture(case);
        let d = SimDesign::lower(&ir, top, &params).unwrap();
        let family = syl_sim::FastFamilyProgram::new(&d).unwrap();
        check_trace(case, &d, 8, Some(&family));
    }
}
