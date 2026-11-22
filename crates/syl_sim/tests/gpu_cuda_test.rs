#[path = "../examples/bench_gpu_2d/support.rs"]
mod support;

use std::process::Command;
use support::*;
use syl_sim::gpu_cuda::{CudaAvailability, KernelMode, cuda_availability, try_generate_cuda};
use syl_sim::{HwDirection, Simulator, StateMode};

#[test]
fn family_code_is_emitted_once_across_lane_counts() {
    let ir = sources();
    for n in [1, 4, 16, 64] {
        let d = lower(&ir, "LaneArray", n);
        let g = try_generate_cuda(&d, options(&d, KernelMode::InstanceFamilyParallel)).unwrap();
        assert_eq!(g.family_size, n as usize);
        assert!(g.source.contains(&format!("family_slot_map[{n}]")));
        assert_eq!(g.source.matches("__device__ void eval_comb(").count(), 1);
        assert_eq!(g.source.matches("Word next0 = ").count(), 1);
        assert!(g.source.contains("f.put("));
    }
}

#[test]
fn device_outputs_and_first_clocked_failure_match_cpu() {
    if cuda_availability() == CudaAvailability::Unavailable {
        return;
    }
    let ir = sources();
    let tmp = Temp::new();
    for (top, size) in [
        ("LaneArray", 1),
        ("LaneArray", 4),
        ("LaneArray", 16),
        ("LaneArray", 64),
        ("SyncFifo", 8),
        ("SyncFifo", 64),
        ("SimpleDualPortRam", 16),
        ("SimpleDualPortRam", 256),
        ("SimpleDualPortRam", 4096),
        ("RoundRobinArbiter4", 1),
    ] {
        let d = lower(&ir, top, size);
        let (batch, cycles) = (7, 23);
        let mut ref_sims = (0..batch)
            .map(|_| Simulator::with_mode(&d, StateMode::TwoState))
            .collect::<Vec<_>>();
        let mut expected = Vec::new();
        for cycle in 1..=cycles {
            for (b, sim) in ref_sims.iter_mut().enumerate() {
                drive(sim, &d, b as u32, cycle);
                sim.tick("clk").unwrap();
            }
            for p in d
                .top_ports()
                .iter()
                .filter(|p| p.direction == HwDirection::Out)
            {
                let width = d.signal(p.signal).width;
                for low in (0..width).step_by(64) {
                    expected.extend(ref_sims.iter().map(|s| {
                        s.read(p.signal)
                            .slice(low, (width - low).min(64))
                            .as_u64()
                            .unwrap()
                    }));
                }
            }
        }
        for mode in [
            KernelMode::BatchPerThread,
            KernelMode::InstanceFamilyParallel,
        ] {
            let bin = build(&tmp.0, &d, mode);
            let probe = Command::new(&bin.path).arg("probe").status().unwrap();
            if probe.code() == Some(77) {
                eprintln!("CUDA device unavailable");
                return;
            }
            assert!(probe.success());
            assert!(bin.compile_ms > 0.0 && bin.gen_ms > 0.0);
            let trace = run(&bin, batch, cycles, "trace");
            assert!(trace["fail_cycle"].is_null());
            let actual: Vec<u64> = serde_json::from_value(trace["samples"].clone()).unwrap();
            assert_eq!(actual.len(), expected.len());
            for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
                assert_eq!(a, e, "{top}({size}) {mode:?} word {i}");
            }
            let final_state = run(&bin, batch, cycles, "final");
            let finals: Vec<u64> = serde_json::from_value(final_state["samples"].clone()).unwrap();
            assert_eq!(
                finals,
                expected[expected.len() - expected.len() / cycles as usize..]
            );
            assert_eq!(final_state["digest"], trace["digest"]);
        }
    }
    let result = fault(&ir, &tmp.0);
    assert!(result["vcd_bytes"].as_u64().unwrap() > 0);
}
