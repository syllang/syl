//! Run a single point: bench_single_trace <engine> <design> <cycles>.
#[path = "bench_single_trace/verilator.rs"]
mod verilator;
#[allow(dead_code)]
#[path = "bench_cpu_2d/workload.rs"]
mod workload;
use std::time::Instant;
use syl_sim::{
    BackendRequest, SimBackendChoice, SimDesign, gpu_cuda::CudaCache, select_local_backend,
};

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let cold = Instant::now();
    let (ir, top, params) = workload::fixture(&args[1]);
    let d = SimDesign::lower(&ir, top, &params).unwrap();
    if args[0] == "emit" {
        verilator::emit(&d, &ir, top, &params, std::path::Path::new(&args[2]));
        return;
    }
    let cycles: u64 = args[2].parse().unwrap();
    let choice = match args[0].as_str() {
        "cpu-jit" => SimBackendChoice::CpuJit,
        "cpu-ref" => SimBackendChoice::CpuRef,
        "cpu-simd" | "family" => SimBackendChoice::CpuSimd,
        _ => panic!("unknown engine"),
    };
    let compile = Instant::now();
    let plan = select_local_backend(
        &d,
        &BackendRequest {
            choice,
            cpu_mode: match args[0].as_str() {
                "cpu-simd" => syl_sim::CpuSimdMode::Batch,
                "family" => syl_sim::CpuSimdMode::Family,
                _ => Default::default(),
            },
            cycles,
            ..Default::default()
        },
        CudaCache::Disabled,
    )
    .unwrap();
    let hot = Instant::now();
    let result = plan.run(0, 1).unwrap();
    let hot_ms = hot.elapsed().as_secs_f64() * 1000.;
    assert!(result.failure.is_none(), "{:?}", result.failure);
    println!(
        "{}",
        serde_json::json!({"compile_ms":(hot-compile).as_secs_f64()*1000.,"cold_ms":(hot-cold).as_secs_f64()*1000.,"hot_ms":hot_ms,"digest":result.digest})
    );
}
