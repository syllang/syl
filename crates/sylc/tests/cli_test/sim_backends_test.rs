use super::*;

const FAULT: &str = include_str!("../../../../examples/benchmarks/fault_hunt_pipeline.syl");
const SEED: &str = "2691343690239311708";

#[test]
fn deterministic_batch_flags_and_legacy_output() {
    let p = TestProject::new();
    p.write("top.syl", "cell Top(i:in UInt<32>)->q:UInt<32>{q:=i}");
    let run = |args: &[&str]| {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_sylc"));
        let out = cmd
            .current_dir(&p.0)
            .env("PATH", "")
            .args(["sim", "top.syl", "--top", "Top"])
            .args(args)
            .output()
            .unwrap();
        success(&out);
        String::from_utf8(out.stdout).unwrap()
    };
    assert_eq!(
        run(&[]),
        "Top cycles=0\nq=00000000000000000000000000000000\n"
    );
    let ref_out = run(&[
        "--backend",
        "cpu-ref",
        "--batch",
        "16",
        "--cycles",
        "7",
        "--seed",
        "123",
    ]);
    let simd = run(&[
        "--backend",
        "auto",
        "--batch",
        "16",
        "--cycles",
        "7",
        "--seed",
        "123",
        "--threads",
        "3",
        "--gpu-mode",
        "batch",
        "--no-gpu-cache",
    ]);
    assert_eq!(ref_out.replace("cpu-ref", "cpu-simd"), simd);
    assert!(simd.starts_with("Top backend=cpu-simd batch=16 cycles=7 seed=123 digest="));
    let cold = run(&[
        "--batch",
        "4096",
        "--cycles",
        "1000",
        "--gpu-cache-dir",
        "cache",
    ]);
    assert!(cold.contains("backend=cpu-simd"));
}

#[test]
fn batch_combinations_and_protected_destinations_report_e_sim() {
    let p = TestProject::new();
    let manifest = p.write("syl.toml", "[paths]\nworkspace_roots=['src']\n");
    let src = p.write("src/top.syl", CLEAN);
    p.write("s.json", "[]");
    fs::hard_link(&manifest, p.0.join("linked.vcd")).unwrap();
    let invoke = |args: &[&str]| {
        let mut a = vec![
            "sim",
            "src/top.syl",
            "--top",
            "Top",
            "--diagnostics-format",
            "json",
        ];
        a.extend(args);
        let out = p.run("", &a);
        failure(&out);
        assert_eq!(report(&out)["diagnostics"][0]["code"], "E_SIM");
    };
    for args in [
        vec!["--batch", "0"],
        vec!["--threads", "0"],
        vec!["--batch", "2", "--stimulus", "s.json"],
        vec!["--batch", "2", "--vcd", "x.vcd"],
        vec!["--no-gpu-cache", "--gpu-cache-dir", "cache"],
        vec!["--replay-vcd", "src/top.syl"],
        vec!["--replay-vcd", "linked.vcd"],
    ] {
        invoke(&args);
    }
    for backend in ["cpu-simd", "cpu-jit", "gpu-cuda"] {
        for flag in [
            vec!["--fail-on-x"],
            vec!["--strict-unique"],
            vec!["--stimulus", "s.json"],
            vec!["--vcd", "x.vcd"],
        ] {
            let mut args = vec!["--backend", backend];
            args.extend(flag);
            invoke(&args);
        }
    }
    assert_eq!(fs::read_to_string(src).unwrap(), CLEAN);
    assert_eq!(
        fs::read_to_string(manifest).unwrap(),
        "[paths]\nworkspace_roots=['src']\n"
    );
}

#[test]
fn gpu_and_simd_replay_identical_fault_wave_and_source_location() {
    let p = TestProject::new();
    p.write("fault.syl", FAULT);
    let mut expected = None;
    for (backend, mode) in [
        ("cpu-simd", "auto"),
        ("cpu-jit", "auto"),
        ("gpu-cuda", "batch"),
        ("gpu-cuda", "family"),
    ] {
        if backend == "gpu-cuda" && syl_sim::gpu_cuda::probe_cuda().is_err() {
            continue;
        }
        let args = [
            "sim",
            "fault.syl",
            "--top",
            "FaultHuntPipeline",
            "--batch",
            "7",
            "--threads",
            "3",
            "--cycles",
            "12",
            "--seed",
            SEED,
            "--backend",
            backend,
            "--gpu-mode",
            mode,
            "--replay-vcd",
            "failure.vcd",
            "--gpu-cache-dir",
            "cache",
            "--diagnostics-format",
            "json",
        ];
        let out = p.run("", &args);
        failure(&out);
        let diag = &report(&out)["diagnostics"][0];
        assert_eq!(diag["code"], "E_SIM");
        assert!(
            diag["message"]
                .as_str()
                .unwrap()
                .contains("batch 0, cycle 5, FaultHuntPipeline.guard.assert[0]")
        );
        let loc = &diag["location"];
        assert_eq!(loc["file"], "fault.syl");
        assert_eq!(
            &FAULT[loc["byte_start"].as_u64().unwrap() as usize
                ..loc["byte_end"].as_u64().unwrap() as usize],
            "assert(not trip)"
        );
        let wave = fs::read_to_string(p.0.join("failure.vcd")).unwrap();
        assert!(wave.contains("$scope module guard $end") && wave.contains("$enddefinitions $end"));
        assert!(wave.contains("0s0") && wave.contains("1s0") && wave.contains("#13"));
        assert_eq!(expected.get_or_insert_with(|| wave.clone()), &wave);
        fs::remove_file(p.0.join("failure.vcd")).unwrap();
    }
}

#[test]
fn cuda_probe_arch_and_cache_behavior_through_cli() {
    let p = TestProject::new();
    p.write("top.syl", CLEAN);
    let run = |args: &[&str], envs: &[(&str, &str)]| {
        Command::new(env!("CARGO_BIN_EXE_sylc"))
            .current_dir(&p.0)
            .args([
                "sim",
                "top.syl",
                "--top",
                "Top",
                "--backend",
                "gpu-cuda",
                "--batch",
                "7",
            ])
            .args(args)
            .envs(envs.iter().copied())
            .output()
            .unwrap()
    };
    let absent = run(&[], &[("PATH", "")]);
    failure(&absent);
    assert!(String::from_utf8_lossy(&absent.stderr).contains("nvcc is unavailable"));
    if syl_sim::gpu_cuda::cuda_availability() == syl_sim::gpu_cuda::CudaAvailability::Unavailable {
        return;
    }
    for arch in ["sm_7", "sm_7500", "sm_75AB", "sm_75 bad", "native1"] {
        let bad = run(&[], &[("SYL_CUDA_ARCH", arch)]);
        failure(&bad);
        assert!(String::from_utf8_lossy(&bad.stderr).contains("invalid SYL_CUDA_ARCH"));
    }
    let hidden = run(
        &[],
        &[("CUDA_VISIBLE_DEVICES", ""), ("SYL_CUDA_ARCH", "sm_75")],
    );
    failure(&hidden);
    assert!(String::from_utf8_lossy(&hidden.stderr).contains("no usable CUDA device"));
    if syl_sim::gpu_cuda::probe_cuda().is_err() {
        return;
    }
    let envs = [("SYL_CUDA_CACHE_DIR", "env-cache")];
    let misses = run(&[], &envs);
    success(&misses);
    let hot = run(&[], &envs);
    success(&hot);
    assert!(String::from_utf8_lossy(&misses.stdout).contains("cache=miss"));
    assert_eq!(
        String::from_utf8_lossy(&misses.stdout).replace("cache=miss", "cache=hit"),
        String::from_utf8_lossy(&hot.stdout)
    );
    let disabled = run(&["--no-gpu-cache"], &envs);
    success(&disabled);
    assert_eq!(misses.stdout, disabled.stdout);
}
