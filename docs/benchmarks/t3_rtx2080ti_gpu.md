# T3 RTX 2080 Ti CUDA simulation prototype

This report records the CUDA code-generation prototype in `crates/syl_sim/src/gpu_cuda.rs`. It generates a persistent multi-cycle kernel from `SimDesign`, uses a structure-of-arrays address of `state[slot_id * total_threads + tid]`, initializes per-thread SplitMix64 input data, and records the first assertion failure as `(batch, cycle, assert_id)`. `KernelMode::BatchPerThread` maps one independent test to one thread. `KernelMode::InstanceFamilyParallel` exposes the `InstanceFamily` lane index and inserts a block barrier for the two-dimensional `(B, N)` mapping.

## Reproduction

```sh
cargo test -p syl_sim --test gpu_cuda_test -- --nocapture
```

The test suite always checks the generated source without a GPU. When `nvcc` is available, it also compiles the generated source with `-O3 -arch=sm_75`. Temporary `.cu` and `.o` files are deleted by the test.

```sh
nvcc --version
nvidia-smi
```

The development machine reported an NVIDIA GeForce RTX 2080 Ti with 11,264 MiB VRAM, driver 615.71.09, CUDA 13.4, and `nvcc` V13.4.92 on November 22, 2026. The measured prototype compile test completed successfully in 0.53 seconds under `cargo test` including Rust test overhead. This is a code-generation compile check rather than a throughput benchmark because the repository has no CUDA runtime launcher or GPU/Rust FFI dependency yet.

## Structured result summary

```json
{
  "gpu": "NVIDIA GeForce RTX 2080 Ti",
  "vram_mib": 11264,
  "driver": "615.71.09",
  "cuda": "13.4",
  "nvcc": "13.4.92",
  "arch": "sm_75",
  "modes": ["batch_per_thread", "instance_family_parallel"],
  "state_layout": "state[slot_id * total_threads + tid]",
  "persistent_cycles": true,
  "device_prng": "SplitMix64",
  "first_failure": ["batch_idx", "cycle", "assert_id"],
  "runtime_matrix": "not available in this prototype"
}
```

The implementation deliberately keeps CUDA optional. Standard CI can run code-generation and source-shape tests without `nvcc`, while a CUDA-equipped machine exercises the compiler path. Full `(B, N)` throughput, H2D/D2H timing, VRAM peaks, CPU replay, and VCD export require a runtime launcher and are therefore left for the next GPU-runtime increment.
