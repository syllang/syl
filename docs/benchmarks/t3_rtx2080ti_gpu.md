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

The development machine reported an NVIDIA GeForce RTX 2080 Ti with 11,264 MiB VRAM, driver 615.71.09, CUDA 13.4, and `nvcc` V13.4.92 on November 22, 2026. The self-contained runner compiles and executes with `nvcc` directly. A complete `LaneArray` matrix was collected with `K=1,000` cycles for `N ∈ {1,4,16,64}` and `B ∈ {1,16,64,256,1024,4096,16384}`. Each row reports kernel milliseconds and aggregate million cycles/s.

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
  "runtime_matrix": "lane_array_n_1_4_16_64_x_batch_1_16_64_256_1024_4096_16384",
  "cycles": 1000
}
```

The benchmark command is `cargo run -p syl_sim --example bench_gpu_2d`. The run produced 56 rows. Representative results are shown below.

| N | B | Mode 1 kernel ms | Mode 1 aggregate Mcycles/s | Mode 2 kernel ms | Mode 2 aggregate Mcycles/s | peak VRAM MiB |
|---:|---:|---:|---:|---:|---:|---:|
| 1 | 1 | 1.058 | 0.95 | 1.080 | 0.93 | 178 |
| 1 | 16,384 | 3.663 | 4,472.63 | 3.752 | 4,366.70 | 184 |
| 4 | 16,384 | 35.118 | 466.54 | 38.778 | 422.51 | 192 |
| 16 | 16,384 | 239.332 | 68.46 | 235.922 | 69.45 | 332 |
| 64 | 16,384 | 1,078.658 | 15.19 | 1,089.100 | 15.04 | 1,186 |

The largest tested case reached about 4.47 billion cycles/s for `N=1`. Family-parallel execution is close to batch-per-thread in this prototype because the generated lane template is scalarized per thread and top-level lane reduction is not yet emitted.

The runner measures H2D and D2H transfers, reports CUDA memory usage, and returns first-failure fields in JSON. CPU `Simulator` differential replay, VCD export, FIFO/RAM/arbiter matrix rows, and per-cycle state-return bandwidth remain follow-up work. The generated runner remains optional and CI without CUDA still passes the source-generation tests.

## Raw benchmark JSON

```json
{"gpu":"RTX 2080 Ti","cycles":1000,"results":[{"n":1,"batch":1,"mode":"BatchPerThread","elapsed_ms":1.05776,"h2d_ms":0.010336,"d2h_ms":0.019552,"peak_vram_mib":178,"wall_ms":908.248},{"n":1,"batch":1,"mode":"InstanceFamilyParallel","elapsed_ms":1.079584,"h2d_ms":0.008416,"d2h_ms":0.020512,"peak_vram_mib":178,"wall_ms":847.903},{"n":1,"batch":16,"mode":"BatchPerThread","elapsed_ms":0.94816,"h2d_ms":0.020576,"d2h_ms":0.020064,"peak_vram_mib":178,"wall_ms":828.906},{"n":1,"batch":16,"mode":"InstanceFamilyParallel","elapsed_ms":0.923072,"h2d_ms":0.02048,"d2h_ms":0.019072,"peak_vram_mib":178,"wall_ms":815.487},{"n":1,"batch":64,"mode":"BatchPerThread","elapsed_ms":0.976896,"h2d_ms":0.025024,"d2h_ms":0.024992,"peak_vram_mib":178,"wall_ms":825.393},{"n":1,"batch":64,"mode":"InstanceFamilyParallel","elapsed_ms":0.994528,"h2d_ms":0.020448,"d2h_ms":0.019264,"peak_vram_mib":178,"wall_ms":810.491},{"n":1,"batch":256,"mode":"BatchPerThread","elapsed_ms":1.795776,"h2d_ms":0.022784,"d2h_ms":0.028928,"peak_vram_mib":178,"wall_ms":838.411},{"n":1,"batch":256,"mode":"InstanceFamilyParallel","elapsed_ms":2.289664,"h2d_ms":0.025024,"d2h_ms":0.02656,"peak_vram_mib":178,"wall_ms":904.328},{"n":1,"batch":1024,"mode":"BatchPerThread","elapsed_ms":1.78176,"h2d_ms":0.02256,"d2h_ms":0.019168,"peak_vram_mib":178,"wall_ms":850.159},{"n":1,"batch":1024,"mode":"InstanceFamilyParallel","elapsed_ms":1.822656,"h2d_ms":0.020832,"d2h_ms":0.019776,"peak_vram_mib":178,"wall_ms":833.994},{"n":1,"batch":4096,"mode":"BatchPerThread","elapsed_ms":1.593344,"h2d_ms":0.032768,"d2h_ms":0.019776,"peak_vram_mib":178,"wall_ms":827.189},{"n":1,"batch":4096,"mode":"InstanceFamilyParallel","elapsed_ms":2.174112,"h2d_ms":0.023552,"d2h_ms":0.019232,"peak_vram_mib":178,"wall_ms":845.234},{"n":1,"batch":16384,"mode":"BatchPerThread","elapsed_ms":3.663168,"h2d_ms":0.022496,"d2h_ms":0.01952,"peak_vram_mib":184,"wall_ms":835.016},{"n":1,"batch":16384,"mode":"InstanceFamilyParallel","elapsed_ms":3.752032,"h2d_ms":0.022432,"d2h_ms":0.019776,"peak_vram_mib":184,"wall_ms":835.128},{"n":4,"batch":1,"mode":"BatchPerThread","elapsed_ms":3.072224,"h2d_ms":0.008384,"d2h_ms":0.02064,"peak_vram_mib":178,"wall_ms":895.235},{"n":4,"batch":1,"mode":"InstanceFamilyParallel","elapsed_ms":3.086336,"h2d_ms":0.010144,"d2h_ms":0.02032,"peak_vram_mib":178,"wall_ms":896.026},{"n":4,"batch":16,"mode":"BatchPerThread","elapsed_ms":2.993856,"h2d_ms":0.02048,"d2h_ms":0.019168,"peak_vram_mib":178,"wall_ms":916.833},{"n":4,"batch":16,"mode":"InstanceFamilyParallel","elapsed_ms":3.005056,"h2d_ms":0.020672,"d2h_ms":0.020736,"peak_vram_mib":178,"wall_ms":946.92},{"n":4,"batch":64,"mode":"BatchPerThread","elapsed_ms":3.741728,"h2d_ms":0.021824,"d2h_ms":0.021024,"peak_vram_mib":178,"wall_ms":925.57},{"n":4,"batch":64,"mode":"InstanceFamilyParallel","elapsed_ms":4.020224,"h2d_ms":0.020832,"d2h_ms":0.020224,"peak_vram_mib":178,"wall_ms":964.056},{"n":4,"batch":256,"mode":"BatchPerThread","elapsed_ms":8.208992,"h2d_ms":0.02048,"d2h_ms":0.020128,"peak_vram_mib":178,"wall_ms":931.149},{"n":4,"batch":256,"mode":"InstanceFamilyParallel","elapsed_ms":8.595168,"h2d_ms":0.021536,"d2h_ms":0.023808,"peak_vram_mib":178,"wall_ms":906.634},{"n":4,"batch":1024,"mode":"BatchPerThread","elapsed_ms":7.760864,"h2d_ms":0.021024,"d2h_ms":0.02,"peak_vram_mib":178,"wall_ms":924.361},{"n":4,"batch":1024,"mode":"InstanceFamilyParallel","elapsed_ms":10.74992,"h2d_ms":0.021824,"d2h_ms":0.02016,"peak_vram_mib":178,"wall_ms":936.011},{"n":4,"batch":4096,"mode":"BatchPerThread","elapsed_ms":8.325504,"h2d_ms":0.021984,"d2h_ms":0.023104,"peak_vram_mib":182,"wall_ms":904.719},{"n":4,"batch":4096,"mode":"InstanceFamilyParallel","elapsed_ms":10.330752,"h2d_ms":0.026816,"d2h_ms":0.022784,"peak_vram_mib":182,"wall_ms":914.224},{"n":4,"batch":16384,"mode":"BatchPerThread","elapsed_ms":35.118305,"h2d_ms":0.03904,"d2h_ms":0.047584,"peak_vram_mib":192,"wall_ms":937.449},{"n":4,"batch":16384,"mode":"InstanceFamilyParallel","elapsed_ms":38.777634,"h2d_ms":0.039168,"d2h_ms":0.021152,"peak_vram_mib":192,"wall_ms":961.828},{"n":16,"batch":1,"mode":"BatchPerThread","elapsed_ms":20.049889,"h2d_ms":0.010528,"d2h_ms":0.0224,"peak_vram_mib":280,"wall_ms":1576.367},{"n":16,"batch":1,"mode":"InstanceFamilyParallel","elapsed_ms":17.874847,"h2d_ms":0.00832,"d2h_ms":0.051168,"peak_vram_mib":280,"wall_ms":1590.137},{"n":16,"batch":16,"mode":"BatchPerThread","elapsed_ms":20.811872,"h2d_ms":0.0208,"d2h_ms":0.023552,"peak_vram_mib":280,"wall_ms":1558.728},{"n":16,"batch":16,"mode":"InstanceFamilyParallel","elapsed_ms":19.183359,"h2d_ms":0.02064,"d2h_ms":0.022112,"peak_vram_mib":280,"wall_ms":1552.877},{"n":16,"batch":64,"mode":"BatchPerThread","elapsed_ms":25.226528,"h2d_ms":0.014688,"d2h_ms":0.027392,"peak_vram_mib":280,"wall_ms":1560.345},{"n":16,"batch":64,"mode":"InstanceFamilyParallel","elapsed_ms":24.364128,"h2d_ms":0.020896,"d2h_ms":0.023968,"peak_vram_mib":280,"wall_ms":1585.537},{"n":16,"batch":256,"mode":"BatchPerThread","elapsed_ms":46.949215,"h2d_ms":0.020672,"d2h_ms":0.026336,"peak_vram_mib":280,"wall_ms":1570.542},{"n":16,"batch":256,"mode":"InstanceFamilyParallel","elapsed_ms":51.25536,"h2d_ms":0.021248,"d2h_ms":0.03888,"peak_vram_mib":280,"wall_ms":1601.138},{"n":16,"batch":1024,"mode":"BatchPerThread","elapsed_ms":69.784195,"h2d_ms":0.02096,"d2h_ms":0.02928,"peak_vram_mib":284,"wall_ms":1599.188},{"n":16,"batch":1024,"mode":"InstanceFamilyParallel","elapsed_ms":74.141693,"h2d_ms":0.021984,"d2h_ms":0.04288,"peak_vram_mib":284,"wall_ms":1622.739},{"n":16,"batch":4096,"mode":"BatchPerThread","elapsed_ms":99.868675,"h2d_ms":0.040576,"d2h_ms":0.033888,"peak_vram_mib":294,"wall_ms":1639.383},{"n":16,"batch":4096,"mode":"InstanceFamilyParallel","elapsed_ms":89.286369,"h2d_ms":0.036864,"d2h_ms":0.027296,"peak_vram_mib":294,"wall_ms":1624.629},{"n":16,"batch":16384,"mode":"BatchPerThread","elapsed_ms":239.332443,"h2d_ms":0.104544,"d2h_ms":0.04704,"peak_vram_mib":332,"wall_ms":1796.367},{"n":16,"batch":16384,"mode":"InstanceFamilyParallel","elapsed_ms":235.921661,"h2d_ms":0.106272,"d2h_ms":0.030464,"peak_vram_mib":332,"wall_ms":1743.21},{"n":64,"batch":1,"mode":"BatchPerThread","elapsed_ms":92.475937,"h2d_ms":0.02144,"d2h_ms":0.049888,"peak_vram_mib":984,"wall_ms":5343.112},{"n":64,"batch":1,"mode":"InstanceFamilyParallel","elapsed_ms":92.272194,"h2d_ms":0.026208,"d2h_ms":0.024864,"peak_vram_mib":984,"wall_ms":5389.204},{"n":64,"batch":16,"mode":"BatchPerThread","elapsed_ms":100.389854,"h2d_ms":0.016384,"d2h_ms":0.051168,"peak_vram_mib":984,"wall_ms":5419.989},{"n":64,"batch":16,"mode":"InstanceFamilyParallel","elapsed_ms":99.757759,"h2d_ms":0.02048,"d2h_ms":0.038144,"peak_vram_mib":984,"wall_ms":5326.068},{"n":64,"batch":64,"mode":"BatchPerThread","elapsed_ms":116.277603,"h2d_ms":0.02048,"d2h_ms":0.05312,"peak_vram_mib":984,"wall_ms":5428.467},{"n":64,"batch":64,"mode":"InstanceFamilyParallel","elapsed_ms":113.355522,"h2d_ms":0.020544,"d2h_ms":0.025248,"peak_vram_mib":984,"wall_ms":5446.707},{"n":64,"batch":256,"mode":"BatchPerThread","elapsed_ms":271.546356,"h2d_ms":0.022336,"d2h_ms":0.04512,"peak_vram_mib":988,"wall_ms":5603.81},{"n":64,"batch":256,"mode":"InstanceFamilyParallel","elapsed_ms":287.25296,"h2d_ms":0.020672,"d2h_ms":0.028896,"peak_vram_mib":988,"wall_ms":5564.408},{"n":64,"batch":1024,"mode":"BatchPerThread","elapsed_ms":325.151031,"h2d_ms":0.04032,"d2h_ms":0.045856,"peak_vram_mib":998,"wall_ms":5572.917},{"n":64,"batch":1024,"mode":"InstanceFamilyParallel","elapsed_ms":327.778595,"h2d_ms":0.03904,"d2h_ms":0.024096,"peak_vram_mib":998,"wall_ms":5641.902},{"n":64,"batch":4096,"mode":"BatchPerThread","elapsed_ms":415.69281,"h2d_ms":0.112384,"d2h_ms":0.047872,"peak_vram_mib":1036,"wall_ms":5726.469},{"n":64,"batch":4096,"mode":"InstanceFamilyParallel","elapsed_ms":397.311188,"h2d_ms":0.108192,"d2h_ms":0.049856,"peak_vram_mib":1036,"wall_ms":5651.521},{"n":64,"batch":16384,"mode":"BatchPerThread","elapsed_ms":1078.657715,"h2d_ms":0.379296,"d2h_ms":0.051296,"peak_vram_mib":1186,"wall_ms":6283.188},{"n":64,"batch":16384,"mode":"InstanceFamilyParallel","elapsed_ms":1089.099731,"h2d_ms":0.376864,"d2h_ms":0.048704,"peak_vram_mib":1186,"wall_ms":6337.905}]}
```
