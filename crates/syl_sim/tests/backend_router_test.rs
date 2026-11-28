#[allow(dead_code)]
#[path = "../examples/bench_gpu_2d/support.rs"]
mod support;
use ResolvedBackend::{CpuJit, CpuRef, CpuSimd, GpuCuda};
use syl_sim::{gpu_cuda::KernelMode::*, *};

#[test]
fn family_and_batch_crossovers_are_host_independent() {
    let ir = support::sources();
    for f in [1, 4, 16, 64] {
        let design = support::lower(&ir, "LaneArray", f);
        for b in [1, 64, 256, 1024, 4096] {
            let mode = if f >= 4 && b <= 256 {
                InstanceFamilyParallel
            } else {
                BatchPerThread
            };
            for hot in [false, true] {
                let request = BackendRequest {
                    batch: b,
                    cycles: 1000,
                    gpu_available: true,
                    gpu_cache_hit: hot,
                    ..Default::default()
                };
                let choose = |r| select_backend(&design, &r).unwrap().backend;
                let expected = if b == 1 {
                    CpuJit
                } else if f >= 16 || b >= 4000 || (hot && b >= 256) {
                    GpuCuda(mode)
                } else {
                    CpuSimd
                };
                assert_eq!(choose(request), expected, "F={f} B={b} hot={hot}");
                assert_eq!(
                    choose(BackendRequest {
                        gpu_available: false,
                        ..request
                    }),
                    if b == 1 { CpuJit } else { CpuSimd }
                );
                for (forced, expected) in [
                    (GpuKernelMode::Auto, mode),
                    (GpuKernelMode::Batch, BatchPerThread),
                    (GpuKernelMode::Family, InstanceFamilyParallel),
                ] {
                    assert_eq!(
                        choose(BackendRequest {
                            choice: SimBackendChoice::GpuCuda,
                            gpu_mode: forced,
                            ..request
                        }),
                        GpuCuda(expected)
                    );
                }
            }
        }
    }
}

#[test]
fn exact_work_thresholds_and_semantic_exclusions() {
    let d = support::lower(&support::sources(), "RoundRobinArbiter4", 1);
    let r = BackendRequest {
        gpu_available: true,
        ..Default::default()
    };
    for (batch, cycles, hot, backend) in [
        (1, 999, false, CpuRef),
        (1, 1000, false, CpuJit),
        (2, 127999, true, CpuSimd),
        (2, 128000, true, GpuCuda(BatchPerThread)),
        (2, 1999999, false, CpuSimd),
        (2, 2000000, false, GpuCuda(BatchPerThread)),
        (2, u64::MAX, false, CpuSimd),
    ] {
        assert_eq!(
            select_backend(
                &d,
                &BackendRequest {
                    batch,
                    cycles,
                    gpu_cache_hit: hot,
                    ..r
                }
            )
            .unwrap()
            .backend,
            backend
        );
    }
    let family = support::lower(&support::sources(), "LaneArray", 16);
    for (batch, cycles, expected) in [
        (256, 999, CpuSimd),
        (256, 1000, GpuCuda(InstanceFamilyParallel)),
        (257, 1000, GpuCuda(BatchPerThread)),
        (1, 50_000, CpuJit),
    ] {
        assert_eq!(
            select_backend(&family, &BackendRequest { batch, cycles, ..r })
                .unwrap()
                .backend,
            expected
        );
    }
    for debug in [
        BackendRequest {
            stimulus: true,
            ..r
        },
        BackendRequest {
            direct_vcd: true,
            ..r
        },
        BackendRequest {
            fail_on_x: true,
            ..r
        },
        BackendRequest {
            strict_unique: true,
            ..r
        },
        BackendRequest {
            state_mode: StateMode::FourState,
            ..r
        },
    ] {
        for choice in [
            SimBackendChoice::Auto,
            SimBackendChoice::CpuRef,
            SimBackendChoice::CpuJit,
            SimBackendChoice::CpuSimd,
            SimBackendChoice::GpuCuda,
        ] {
            let s = select_backend(
                &d,
                &BackendRequest {
                    choice,
                    cycles: 4_000_000,
                    ..debug
                },
            );
            if matches!(
                choice,
                SimBackendChoice::CpuJit | SimBackendChoice::CpuSimd | SimBackendChoice::GpuCuda
            ) {
                assert!(s.is_err());
            } else {
                assert_eq!(s.unwrap().backend, CpuRef);
            }
        }
    }
    assert!(
        select_backend(
            &d,
            &BackendRequest {
                gpu_available: false,
                choice: SimBackendChoice::GpuCuda,
                ..r
            }
        )
        .is_err()
    );
}

#[test]
fn incompatible_ir_preserves_reference_fallback_and_explicit_errors() {
    let mut d = support::lower(&support::sources(), "LaneArray", 4);
    d.resets[0].mode = HwResetMode::Async;
    let r = BackendRequest {
        batch: 4096,
        cycles: 1000,
        gpu_available: true,
        ..Default::default()
    };
    assert_eq!(select_backend(&d, &r).unwrap().backend, CpuRef);
    for choice in [SimBackendChoice::GpuCuda, SimBackendChoice::CpuSimd] {
        assert!(select_backend(&d, &BackendRequest { choice, ..r }).is_err());
    }
    // Variable shifts are supported by FastProgram and rejected by CUDA.
    let d = SimDesign::lower(
        &support::compile(&[(
            "shift",
            "cell Shift(i:in UInt<8>,s:in UInt<3>)->y:UInt<8>{y:=i << s}",
        )]),
        "Shift",
        &Default::default(),
    )
    .unwrap();
    assert_eq!(select_backend(&d, &r).unwrap().backend, CpuSimd);
    assert!(
        select_backend(
            &d,
            &BackendRequest {
                choice: SimBackendChoice::GpuCuda,
                ..r
            }
        )
        .is_err()
    );
}
