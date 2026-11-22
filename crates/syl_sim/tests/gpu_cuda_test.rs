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
        let batch = 7;
        let deep_ram = top == "SimpleDualPortRam" && size >= 256;
        let cycles = if deep_ram { 256 } else { 23 };
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
        if deep_ram {
            assert!(
                expected.iter().any(|&word| word != 0),
                "no observable write for RAM depth {size}"
            );
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
    let fault_design = lower(&ir, "FaultProbe", 1);
    let shared = build(&tmp.0, &fault_design, KernelMode::InstanceFamilyParallel);
    let capture = run(&shared, 4096, 1000, "summary");
    assert_eq!(
        (
            capture["fail_batch"].as_u64(),
            capture["fail_cycle"].as_u64(),
            capture["fail_assert"].as_u64()
        ),
        (Some(173), Some(38), Some(0))
    );
    let result = fault(&ir, &tmp.0);
    assert!(result["vcd_bytes"].as_u64().unwrap() > 0);
}

#[test]
fn launch_options_reject_out_of_range_before_compilation() {
    use syl_sim::gpu_cuda::{RunnerOptions, run_cuda};
    let d = syl_sim::SimDesign::default();
    for (options, field) in [
        (
            RunnerOptions {
                cycles: 4294967297,
                ..Default::default()
            },
            "cycles",
        ),
        (
            RunnerOptions {
                batch: usize::MAX,
                ..Default::default()
            },
            "batch",
        ),
        (
            RunnerOptions {
                block_size: usize::MAX,
                ..Default::default()
            },
            "block",
        ),
    ] {
        let error = run_cuda(&d, Default::default(), options, "unused").unwrap_err();
        assert!(error.contains(field), "{error}");
    }
}

#[test]
fn runner_validates_cli_and_runs_a_bare_filename() {
    use syl_sim::gpu_cuda::{RunnerOptions, run_cuda};
    // Run the Rust API in a child with a private cwd; tests share their process cwd.
    if std::env::var_os("SYL_CUDA_BASENAME_CHILD").is_some() {
        let d = lower(&sources(), "RoundRobinArbiter4", 1);
        let r = run_cuda(
            &d,
            Default::default(),
            RunnerOptions {
                cycles: 7,
                ..Default::default()
            },
            "just-a-basename",
        )
        .unwrap();
        assert_eq!(r.cycles, 7);
        return;
    }
    if cuda_availability() == CudaAvailability::Unavailable {
        return;
    }
    let tmp = Temp::new();
    let d = lower(&sources(), "RoundRobinArbiter4", 1);
    let bin = build(&tmp.0, &d, KernelMode::BatchPerThread);
    let probe = Command::new(&bin.path).arg("probe").status().unwrap();
    if probe.code() == Some(77) {
        return;
    }
    assert!(probe.success());
    for (args, field) in [
        (["1", "4294967297", "128"], "cycles"),
        (["1", "18446744073709551616", "128"], "cycles"),
        (["4294967297", "1", "128"], "batch"),
        (["2147483648", "1", "128"], "batch"),
        (["1", "1", "4294967297"], "block"),
        (["1", "-1", "128"], "cycles"),
        (["1", "1x", "128"], "cycles"),
    ] {
        let out = Command::new(&bin.path).args(args).output().unwrap();
        assert_eq!(out.status.code(), Some(2));
        assert!(String::from_utf8_lossy(&out.stderr).contains(field));
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "runner_validates_cli_and_runs_a_bare_filename",
            "--nocapture",
        ])
        .env("SYL_CUDA_BASENAME_CHILD", "1")
        .current_dir(&tmp.0)
        .output()
        .unwrap();
    assert!(
        child.status.success(),
        "{}",
        String::from_utf8_lossy(&child.stderr)
    );
}

#[test]
fn family_ram_and_parent_assertions_match_cpu() {
    use std::collections::BTreeMap;
    use syl_sim::{ParamValue, RuntimeError, SimDesign};
    let ir = compile(&[("checked", include_str!("gpu_cuda_ram_test.syl"))]);
    let tmp = Temp::new();
    for (member, parent) in [(256, 256), (21, 256), (256, 21)] {
        let params = BTreeMap::from([
            ("MEMBER_BAD".into(), ParamValue::Nat(member)),
            ("PARENT_BAD".into(), ParamValue::Nat(parent)),
        ]);
        let d = SimDesign::lower(&ir, "CheckedArray", &params).unwrap();
        let mode = KernelMode::InstanceFamilyParallel;
        assert_eq!(
            try_generate_cuda(&d, options(&d, mode))
                .unwrap()
                .family_size,
            4
        );
        if cuda_availability() == CudaAvailability::Unavailable {
            continue;
        }
        let bin = build(&tmp.0, &d, mode);
        let probe = Command::new(&bin.path).arg("probe").status().unwrap();
        if probe.code() == Some(77) {
            return;
        }
        assert!(probe.success());
        let (batch, cycles) = (7, 256);
        let trace = run(&bin, batch, cycles, "trace");
        let actual: Vec<u64> = serde_json::from_value(trace["samples"].clone()).unwrap();
        let ports = d
            .top_ports()
            .iter()
            .filter(|p| p.direction == HwDirection::Out)
            .collect::<Vec<_>>();
        assert_eq!(actual.len(), batch as usize * cycles as usize * ports.len());
        let mut failures = Vec::new();
        for b in 0..batch {
            let mut cpu = Simulator::with_mode(&d, StateMode::TwoState);
            for cycle in 1..=cycles {
                drive(&mut cpu, &d, b, cycle);
                match cpu.tick("clk") {
                    Ok(()) => {}
                    Err(RuntimeError::Assertion(a)) => {
                        let id = d
                            .assertions
                            .iter()
                            .position(|p| p.source.path == a.path)
                            .unwrap();
                        failures.push((a.cycle, u64::from(b), id as u64));
                        break;
                    }
                    Err(e) => panic!("{e}"),
                }
                for (i, p) in ports.iter().enumerate() {
                    let idx =
                        ((cycle - 1) as usize * ports.len() + i) * batch as usize + b as usize;
                    assert_eq!(
                        actual[idx],
                        cpu.read(p.signal).as_u64().unwrap(),
                        "batch {b}, cycle {cycle}, {}",
                        p.name
                    );
                }
            }
        }
        if member == 256 && parent == 256 {
            assert!(trace["fail_cycle"].is_null());
            assert!((0..4).all(|lane| actual.iter().any(|word| ((word >> (8 * lane)) & 255) != 0)));
        } else {
            let (c, b, id) = failures.into_iter().min().expect("fixture must trip");
            assert_eq!(
                (
                    trace["fail_cycle"].as_u64(),
                    trace["fail_batch"].as_u64(),
                    trace["fail_assert"].as_u64()
                ),
                (Some(c), Some(b), Some(id))
            );
            let path = &d.assertions[id as usize].source.path;
            assert_eq!(path.contains(".lanes["), member != 256, "{path}");
            if member != 256 {
                assert!(
                    !path.contains("lanes[0]"),
                    "expected a non-representative lane: {path}"
                );
            }
        }
    }
}
