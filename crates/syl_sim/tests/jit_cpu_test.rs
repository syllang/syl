#[allow(dead_code)]
#[path = "../examples/bench_gpu_2d/support.rs"]
mod support;
#[allow(dead_code)]
#[path = "../examples/bench_cpu_2d/workload.rs"]
mod workload;

#[path = "fixtures/shapes_test.rs"]
mod shapes;
#[path = "fixtures/trace_test.rs"]
mod trace;
use syl_sim::*;

#[test]
fn every_settled_output_limb_matches_at_zero_and_after_each_rising_edge() {
    for case in [
        "RoundRobinArbiter4",
        "SimpleDualPortRam",
        "SyncFifo-D8",
        "SyncFifo-D64",
        "LaneArray-N1",
        "LaneArray-N4",
        "LaneArray-N16",
        "LaneArray-N64",
        "WideAluArray",
        "DeepFsmChain",
        "SharedRamArbiter",
        "FaultHuntPipeline",
    ] {
        let d = if let Some((top, src)) = shapes::SHAPES.iter().find(|(top, _)| *top == case) {
            SimDesign::lower(
                &support::compile(&[("fixture", src)]),
                top,
                &Default::default(),
            )
            .unwrap()
        } else {
            let (ir, top, params) = workload::fixture(case);
            SimDesign::lower(&ir, top, &params).unwrap()
        };
        let family = FastFamilyProgram::new(&d).ok();
        assert_eq!(
            family.is_some(),
            matches!(
                case,
                "LaneArray-N4" | "LaneArray-N16" | "LaneArray-N64" | "WideAluArray"
            )
        );
        trace::check_trace(case, &d, 16, family.as_ref());
    }
}

#[test]
fn family_ram_and_global_assertion_order_survive_lane_permutation() {
    let source = r#"
cell Member<D:Domain>(clk:in Clock<D>,rst:in Reset<D>,addr:in UInt<3>,v:in UInt<8>,q:out UInt<8>){
    mem ram:[5] UInt<8>
    signal read:UInt<8>
    mem_write(ram,clk,(addr eq addr),addr,v)
    mem_read_sync(ram,clk,(addr eq addr),addr,read)
    assert(not (v eq 0)) assert(v eq 0)
    q:=read
}
cell Top<D:Domain>(clk:in Clock<D>,rst:in Reset<D>,a:in [4] UInt<3>,v:in [4] UInt<8>,q:out [4] UInt<8>){
    let cells=for i in 0..4 { place Member<D>(clk:clk,rst:rst,addr:a[i],v:v[i],q:q[i]) }
}"#;
    let mut d = SimDesign::lower(
        &support::compile(&[("fault", source)]),
        "Top",
        &Default::default(),
    )
    .unwrap();
    let family = d
        .families
        .iter_mut()
        .find(|f| f.members.len() == 4)
        .unwrap();
    family.members.reverse();
    let native = JitProgram::new(&d).unwrap();
    let simd = FastFamilyProgram::new(&d).unwrap();
    for cycles in [0, 1, 2, 9, 32] {
        let o = BatchOptions {
            cycles,
            seed: 13,
            ..Default::default()
        };
        let r = run_batch(&d, ResolvedBackend::CpuRef, o).unwrap();
        let trace = native.run_trajectory(o.seed, 0, cycles);
        assert_eq!(trace.outputs, [r.digest]);
        assert_eq!(
            trace.failure,
            r.failure.map(|(f, _)| (f.cycle, f.assert_id))
        );
        assert_eq!(simd.run_trajectory(o.seed, cycles), trace);
    }
}

#[test]
fn clock_as_comb_data_and_changed_input_sample_the_correct_edge() {
    let (ir, top, p) = workload::fixture("Fault");
    let mut d = SimDesign::lower(&ir, top, &p).unwrap();
    let clk = d.clocks[0].signal;
    let reg = d.regs[0].signal;
    // Feed the top output from the clock and sample that output on the rising edge.
    let y = d.top_ports().iter().find(|p| p.name == "y").unwrap().signal;
    d.comb
        .iter_mut()
        .find(|a| a.target.signal == y)
        .unwrap()
        .value = SimExpr::Read(SimTarget {
        signal: clk,
        selectors: vec![],
    });
    d.regs[0].next = SimExpr::Read(SimTarget {
        signal: y,
        selectors: vec![],
    });
    // Expose the register too, so a missed high-phase settle is observable.
    d.instances[0].ports.push(SimPort {
        signal: reg,
        name: "stored".into(),
        direction: HwDirection::Out,
    });
    let p = FastProgram::new(&d).unwrap();
    assert!(p.clock_used_in_comb);
    let jit = JitProgram::new(&d).unwrap();
    let mut sim = FastBatchCpuSimulator::new(&p, 1, 1);
    sim.for_each_tile(1, |_, t| {
        assert_eq!(t.word(&p, y, 0), [0]);
        t.tick(&p).unwrap();
        assert_eq!(t.word(&p, y, 0), [0]);
        assert_eq!(t.word(&p, reg, 0), [1]);
    });
    for cycles in [0, 1, 2, 3] {
        let trace = jit.run_trajectory(0, 0, cycles);
        let r = run_batch(
            &d,
            ResolvedBackend::CpuRef,
            BatchOptions {
                cycles,
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(trace.outputs[0], 0);
        assert_eq!(trace.outputs[1] * 3, r.digest);
    }
}

#[test]
fn parent_and_member_ram_sample_old_state_before_either_commits() {
    let ir = support::compile(&[("checked", include_str!("gpu_cuda_ram_test.syl"))]);
    for (member, parent) in [(256, 256), (0, 256), (256, 0), (0, 0)] {
        let params = [
            ("MEMBER_BAD".into(), ParamValue::Nat(member)),
            ("PARENT_BAD".into(), ParamValue::Nat(parent)),
        ]
        .into();
        let d = SimDesign::lower(&ir, "CheckedArray", &params).unwrap();
        let family = FastFamilyProgram::new(&d).unwrap();
        let jit = JitProgram::new(&d).unwrap();
        for cycles in [0, 1, 2, 5, 31] {
            let trace = family.run_trajectory(11, cycles);
            let r = run_batch(
                &d,
                ResolvedBackend::CpuRef,
                BatchOptions {
                    cycles,
                    seed: 11,
                    ..Default::default()
                },
            )
            .unwrap();
            let sum = trace.outputs.iter().enumerate().fold(0u64, |s, (i, w)| {
                s.wrapping_add(w.wrapping_mul(2 * i as u64 + 1))
            });
            assert_eq!(
                trace.failure,
                r.failure.map(|(c, _)| (c.cycle, c.assert_id))
            );
            assert_eq!(sum, r.digest);
            assert_eq!(trace, jit.run_trajectory(11, 0, cycles));
        }
    }
}

#[test]
fn compact_priority_select_matches_reference_for_every_input_pair() {
    let ir = support::compile(&[(
        "small",
        r#"cell Small(a:in UInt<2>,b:in UInt<2>)->y:UInt<2>{
        y:=select priority {a eq 2 => b xor 3, b eq a => 0, b eq 0 => a, default => b+1,}
    }"#,
    )]);
    let d = SimDesign::lower(&ir, "Small", &Default::default()).unwrap();
    let p = FastProgram::new(&d).unwrap();
    let jit = JitProgram::new(&d).unwrap();
    let mut seeds = [None; 16];
    for seed in 0..2000 {
        let a = batch_stimulus_word(seed, 0, 1, 0, 0) & 3;
        let b = batch_stimulus_word(seed, 0, 1, 1, 0) & 3;
        seeds[(4 * a + b) as usize] = Some(seed);
        if seeds.iter().all(Option::is_some) {
            break;
        }
    }
    let mut sim = FastBatchCpuSimulator::new(&p, 16, 16);
    let [a, b, y] = d.top_ports() else {
        unreachable!()
    };
    sim.for_each_tile(1, |_, t| {
        let mut native = jit.simulator();
        for i in 0..16 {
            t.input_word(&p, a.signal, 0)[i] = (i / 4) as u64;
            t.input_word(&p, b.signal, 0)[i] = (i % 4) as u64;
        }
        t.eval_comb(&p);
        for (i, seed) in seeds.into_iter().map(Option::unwrap).enumerate() {
            let r = run_batch(
                &d,
                ResolvedBackend::CpuRef,
                BatchOptions {
                    seed,
                    ..Default::default()
                },
            )
            .unwrap();
            assert_eq!(native.run_trajectory(seed, 0, 1).outputs, [r.digest]);
            assert_eq!(t.word(&p, y.signal, 0)[i], r.digest);
        }
    });
}
