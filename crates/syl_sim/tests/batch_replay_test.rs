#[allow(dead_code)]
#[path = "../examples/bench_gpu_2d/support.rs"]
mod support;
use syl_sim::{
    gpu_cuda::{self, CudaCache, KernelMode::*},
    *,
};

const SHAPES: [(&str, &str); 4] = [
    (
        "WideAluArray",
        include_str!("../../../examples/benchmarks/wide_alu_array.syl"),
    ),
    (
        "DeepFsmChain",
        include_str!("../../../examples/benchmarks/deep_fsm_chain.syl"),
    ),
    (
        "SharedRamArbiter",
        include_str!("../../../examples/benchmarks/shared_ram_arbiter.syl"),
    ),
    (
        "FaultHuntPipeline",
        include_str!("../../../examples/benchmarks/fault_hunt_pipeline.syl"),
    ),
];
const FAULT_SEED: u64 = 2691343690239311708;
fn design(top: &str, source: &str) -> SimDesign {
    SimDesign::lower(
        &support::compile(&[("test", source)]),
        top,
        &Default::default(),
    )
    .unwrap()
}

#[test]
fn final_words_and_earliest_assertion_agree_for_all_four_shapes() {
    let device = match gpu_cuda::probe_cuda() {
        Ok(()) => true,
        Err(e) if e.contains("no usable CUDA device") || e.contains("nvcc is unavailable") => {
            eprintln!("{e}");
            false
        }
        Err(e) => panic!("CUDA probe failed: {e}"),
    };
    let cache = tempfile::tempdir().unwrap();
    for (top, src) in SHAPES {
        let d = design(top, src);
        for (iteration, seed) in [0, FAULT_SEED, u64::MAX].into_iter().enumerate() {
            let o = BatchOptions {
                batch: 7,
                cycles: 12,
                seed,
                cache: CudaCache::Directory(cache.path()),
                ..Default::default()
            };
            let expected = run_batch(&d, ResolvedBackend::CpuRef, o).unwrap();
            for threads in [1, 3] {
                let fast =
                    run_batch(&d, ResolvedBackend::CpuSimd, BatchOptions { threads, ..o }).unwrap();
                assert_eq!(fast, expected, "{top} seed={seed} threads={threads}");
            }
            if top == "FaultHuntPipeline" && seed == FAULT_SEED {
                let (coordinate, f) = expected.failure.as_ref().unwrap();
                assert_eq!(
                    (coordinate.batch, coordinate.cycle, coordinate.assert_id.0),
                    (0, 5, 0)
                );
                assert_eq!(&src[f.span.start..f.span.end], "assert(not trip)");
                assert_eq!(f.path, "FaultHuntPipeline.guard.assert[0]");
            } else {
                assert!(expected.failure.is_none(), "{top} {seed}");
            }
            if device {
                for mode in [BatchPerThread, InstanceFamilyParallel] {
                    let gpu = run_batch(&d, ResolvedBackend::GpuCuda(mode), o).unwrap();
                    assert_eq!(gpu.digest, expected.digest, "{top} {mode:?} seed={seed}");
                    assert_eq!(gpu.failure, expected.failure);
                    assert_eq!(gpu.cache_hit, Some(iteration > 0));
                    assert_eq!(gpu.compile_ms == 0., iteration > 0);
                    eprintln!(
                        "{top} {mode:?} seed={seed} digest={} hit={} compile_ms={:.2} kernel_ms={:.4}",
                        gpu.digest,
                        iteration > 0,
                        gpu.compile_ms,
                        gpu.kernel_ms
                    );
                }
            }
        }
    }
}

#[test]
fn parallel_reduction_orders_by_cycle_then_lane_then_assertion() {
    let src = "cell T<D:Domain>(clk:in Clock<D>,rst:in Reset<D>,i:in Bit)->q:Bit {reg v:Bit reset(rst,0) next v:=i assert(not v) assert(not i) q:=v}";
    let d = design("T", src);
    let o = BatchOptions {
        batch: 257,
        cycles: 8,
        seed: 123,
        threads: 4,
        ..Default::default()
    };
    let reference = run_batch(&d, ResolvedBackend::CpuRef, o).unwrap();
    let fast = run_batch(&d, ResolvedBackend::CpuSimd, o).unwrap();
    assert_eq!(reference, fast);
    let (c, f) = fast.failure.unwrap();
    let mut bytes = vec![];
    assert_eq!(replay_failure(&d, o, c, Some(&mut bytes)).unwrap(), f);
    let wave = String::from_utf8(bytes).unwrap();
    assert!(wave.contains("$enddefinitions $end") && wave.contains("1s0") && wave.contains("0s0"));
    assert!(
        replay_failure(
            &d,
            o,
            FailureCoordinate {
                assert_id: AssertId(c.assert_id.0 ^ 1),
                ..c
            },
            None
        )
        .is_err()
    );
}

#[test]
fn concurrent_compilers_publish_one_complete_runner_and_uncached_plans_stay_cold() {
    if gpu_cuda::probe_cuda().is_err() {
        return;
    }
    let (top, src) = SHAPES[2];
    let d = design(top, src);
    let cache = tempfile::tempdir().unwrap();
    let r = BackendRequest {
        choice: SimBackendChoice::GpuCuda,
        batch: 4,
        cycles: 11,
        ..Default::default()
    };
    let plan = select_local_backend(&d, &r, CudaCache::Directory(cache.path())).unwrap();
    let barrier = std::sync::Barrier::new(2);
    let run = || {
        barrier.wait();
        plan.run(17, 1).unwrap()
    };
    let (a, b) = std::thread::scope(|s| {
        let other = s.spawn(run);
        (run(), other.join().unwrap())
    });
    let hot = plan.run(17, 1).unwrap();
    assert_eq!(a.digest, b.digest);
    assert_eq!(hot.digest, a.digest);
    assert_eq!(hot.cache_hit, Some(true));
    assert_eq!(hot.compile_ms, 0.);
    assert!(a.cache_hit == Some(false) || b.cache_hit == Some(false));
    let isolated = select_local_backend(&d, &r, CudaCache::Disabled).unwrap();
    for _ in 0..2 {
        let result = isolated.run(17, 1).unwrap();
        assert_eq!(result.cache_hit, Some(false));
        assert_eq!(result.digest, hot.digest);
    }
}
