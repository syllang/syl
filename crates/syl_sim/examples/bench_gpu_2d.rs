//! Emit one JSON record per measurement. Pass a VCD filename to keep the replay.
#[path = "bench_gpu_2d/support.rs"]
mod support;
use serde_json::json;
use std::time::Instant;
use support::*;
use syl_sim::gpu_cuda::{CudaAvailability, KernelMode, cuda_availability};

fn main() {
    if cuda_availability() == CudaAvailability::Unavailable {
        eprintln!("nvcc unavailable");
        return;
    }
    let ir = sources();
    let dir = Temp::new();
    let modes = [
        KernelMode::BatchPerThread,
        KernelMode::InstanceFamilyParallel,
    ];
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
        let start = Instant::now();
        let design = lower(&ir, top, size);
        let lower_ms = ms(start.elapsed());
        let mut digests = Vec::new();
        for (index, mode) in modes.iter().enumerate() {
            // Scalar standard-library tops have a singleton family. Use the batch mapping for throughput.
            if index == 1 && top != "LaneArray" {
                continue;
            }
            let bin = build(&dir.0, &design, *mode);
            for (i, b) in [1, 16, 64, 256, 1024, 4096, 16384].into_iter().enumerate() {
                let r = run(&bin, b, 1000, "summary");
                assert!(r["fail_cycle"].is_null());
                if index == 0 {
                    digests.push(r["digest"].clone());
                } else {
                    assert_eq!(r["digest"], digests[i]);
                }
                println!(
                    "{}",
                    json!({"case":top,"size":size,"mode":index+1,"lower_ms":lower_ms,"gen_ms":bin.gen_ms,"cold_ms":bin.compile_ms,"run":r})
                );
            }
            if top == "LaneArray" && size == 16 && index == 0 {
                let mut digest = None;
                for observation in ["summary", "each"] {
                    let r = run(&bin, 16384, 128, observation);
                    if let Some(previous) = digest {
                        assert_eq!(r["digest"], previous);
                    }
                    digest = Some(r["digest"].clone());
                    println!(
                        "{}",
                        json!({"case":"Observation","size":size,"mode":observation,"run":r})
                    );
                }
            }
        }
    }
    println!("{}", fault(&ir, &dir.0));
    if let Some(path) = std::env::args_os().nth(1) {
        std::fs::copy(dir.0.join("failure.vcd"), path).unwrap();
    }
}
