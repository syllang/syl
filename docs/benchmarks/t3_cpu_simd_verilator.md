# T3 CPU baselines on Ryzen 9 7950X

At B=4096, the fastest measured Verilator configuration ran LaneArray-N64 at 11.98 million DUT cycles/s. The fastest SoA configuration on that design reached 2.60 million. On SimpleDualPortRam, SoA reached 969 million and Verilator reached 170 million. One cycle advances one complete design instance, including all N internal lanes.

## Reproduce

```sh
python3 crates/syl_sim/examples/bench_cpu_2d/run.py
cargo test -p syl_sim --test batch_cpu_test
```

The November 22, 2026 run used one AMD Ryzen 9 7950X socket, 16 physical cores and 32 logical CPUs, Linux 7.2.6, rustc 1.96.0 (LLVM 22.1.2), GCC 16.2.1 and Verilator 5.052. Rust used release mode with `-Ctarget-cpu=native`. The C++ harness used `-std=c++20 -O3 -march=native`. Disassembly of `syl_sim::fast_cpu::kernel::execute` in the release example shows eight-wide `vpaddq` and `vpandq` instructions using ZMM registers.

The T2 reference, SoA compiler and SV emitter consume the same specialized design inputs. SoA compiles the scheduled combinational logic and rising-edge updates into flat instruction arrays. Each tile stores up to 64 independent trajectories per word plane. Native workers own disjoint tiles and synchronize once at startup. The integration test counts heap allocations on each executing thread during stimulus, combinational evaluation and ticking. It recorded zero in the measured regions for all seven specializations tested.

Each throughput design uses two fresh Verilator builds with `--cc --exe --build -j 16 -O3 --assert`, `OPT_FAST=-O3` and `--threads 1` or `2`. We tested one host worker and up to 16 and 32 total threads across independent model instances. In the table, `vIxW` denotes I internal threads per model and W host workers. The Verilator column selects the highest measured median throughput for each point. The depth-64 FIFO emits a width-extension diagnostic comparing its seven-bit count with the constant parameter DEPTH. The build retains that diagnostic. [Complete samples and host identification](t3_cpu_2d.json)

## Measurements

All engines use counter-based SplitMix64 keyed by cycle, trajectory index, port and limb, including the high limbs of wide buses. The first edge asserts reset where present. Every configuration produced the same final digest over all output words and trajectories at each of the 48 points. The integration test separately compares all output bits against T2 before and after every one of 32 random edges on 17 trajectories, with one and four requested workers.

Each cell gives **cold setup ms / hot elapsed ms / aggregate million cycles per second**. Rust cold setup includes source elaboration, specialization and batch allocation, plus instruction compilation for SoA. Verilator cold setup includes emission, model build and process-local setup. The bench executable itself was prebuilt. For B=1 the third number gives the single-trajectory rate. For larger B, aggregate throughput is B times the per-trajectory mean progress.

Hot time covers stimulus, clock edges and the final digest. SoA measures from the earliest tile start after the worker barrier to the last tile completion. Verilator measures between the model-ready and digest-complete barriers. The scalar timer surrounds the corresponding loop and digest. Worker creation, join and model destruction fall outside hot timing. The JSON also records process wall time. Each sub-250 ms first observation triggered two more fresh-process samples and the table uses the median. Longer observations were measured once.

Small points use 1,024 cycles per trajectory. Large LaneArray cases use 16 to 256 cycles to bound the serial reference runtime, so those rows describe short traces at the stated B. There was no excluded cycle warm-up or CPU pinning. OS and compiler caches stayed enabled and other jobs were active on the workstation. Sub-millisecond differences and small rankings should be read alongside the raw samples.

| Design | B | Cycles | Scalar Simulator | SoA 1T | SoA 16T | SoA 32T | Verilator 5.052 | Config |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| LaneArray-N1 | 1 | 1024 | 2.011 / 18.79 / 0.05449 | 1.669 / 0.739 / 1.386 | 1.42 / 0.7225 / 1.417 | 1.362 / 0.7341 / 1.395 | 5137 / 0.08439 / 12.13 | v1x1 |
| LaneArray-N1 | 16 | 1024 | 1.587 / 285.4 / 0.0574 | 1.447 / 1.038 / 15.78 | 2.814 / 1.759 / 9.314 | 1.912 / 2.251 / 7.28 | 5138 / 0.191 / 85.76 | v1x16 |
| LaneArray-N1 | 64 | 1024 | 1.72 / 1180 / 0.05553 | 1.752 / 3.041 / 21.55 | 2.338 / 2.391 / 27.41 | 3.803 / 5.256 / 12.47 | 5138 / 1.328 / 49.37 | v1x16 |
| LaneArray-N1 | 256 | 1024 | 2.837 / 4908 / 0.05341 | 1.794 / 13.67 / 19.17 | 2.419 / 6.827 / 38.4 | 3.619 / 8.41 / 31.17 | 5138 / 2.51 / 104.4 | v1x32 |
| LaneArray-N1 | 1024 | 1024 | 2.881 / 1.923e+04 / 0.05454 | 1.833 / 46.76 / 22.42 | 1.937 / 6.711 / 156.3 | 2.217 / 6.908 / 151.8 | 5138 / 7.599 / 138 | v1x32 |
| LaneArray-N1 | 4096 | 256 | 6.44 / 1.848e+04 / 0.05674 | 2.403 / 46.16 / 22.71 | 2.793 / 5.081 / 206.4 | 3.22 / 5.698 / 184 | 5138 / 9.149 / 114.6 | v1x32 |
| LaneArray-N4 | 1 | 1024 | 1.774 / 73.17 / 0.01399 | 1.667 / 3.062 / 0.3344 | 1.786 / 2.929 / 0.3496 | 1.53 / 2.925 / 0.3501 | 5181 / 0.1275 / 8.03 | v1x1 |
| LaneArray-N4 | 16 | 1024 | 1.515 / 1166 / 0.01405 | 1.541 / 5.147 / 3.183 | 2.06 / 9.886 / 1.657 | 2.435 / 9.52 / 1.721 | 5181 / 0.2204 / 74.34 | v1x16 |
| LaneArray-N4 | 64 | 1024 | 1.921 / 4586 / 0.01429 | 1.844 / 17.74 / 3.695 | 1.795 / 5.217 / 12.56 | 2.057 / 17.87 / 3.668 | 5182 / 1.142 / 57.37 | v1x32 |
| LaneArray-N4 | 256 | 1024 | 2.643 / 1.826e+04 / 0.01436 | 1.612 / 50.29 / 5.213 | 2.42 / 20.2 / 12.98 | 2.571 / 27.32 / 9.595 | 5182 / 2.459 / 106.6 | v1x32 |
| LaneArray-N4 | 1024 | 256 | 5.516 / 1.857e+04 / 0.01412 | 1.592 / 53.21 / 4.926 | 2.246 / 12.14 / 21.6 | 2.758 / 8.742 / 29.99 | 5182 / 2.48 / 105.7 | v1x32 |
| LaneArray-N4 | 4096 | 64 | 16.53 / 1.837e+04 / 0.01427 | 2.018 / 51.84 / 5.057 | 2.287 / 5.304 / 49.43 | 2.721 / 5.931 / 44.2 | 5182 / 2.312 / 113.4 | v1x32 |
| LaneArray-N16 | 1 | 1024 | 1.927 / 286.7 / 0.003572 | 1.912 / 11.54 / 0.08876 | 1.943 / 10.76 / 0.09518 | 1.871 / 10.77 / 0.09506 | 5072 / 0.3287 / 3.116 | v1x1 |
| LaneArray-N16 | 16 | 1024 | 1.971 / 4704 / 0.003483 | 2.355 / 22.64 / 0.7236 | 3.181 / 54.04 / 0.3032 | 2.693 / 32.35 / 0.5064 | 5072 / 0.8483 / 19.31 | v1x16 |
| LaneArray-N16 | 64 | 1024 | 3.172 / 1.82e+04 / 0.003601 | 3.405 / 81.01 / 0.809 | 2.671 / 20.62 / 3.178 | 3.319 / 60.08 / 1.091 | 5072 / 2.179 / 30.08 | v1x32 |
| LaneArray-N16 | 256 | 256 | 5.571 / 1.796e+04 / 0.003648 | 1.881 / 54.78 / 1.196 | 2.226 / 19.05 / 3.441 | 3.278 / 23.05 / 2.843 | 5072 / 1.601 / 40.94 | v1x32 |
| LaneArray-N16 | 1024 | 64 | 14.65 / 1.807e+04 / 0.003627 | 2.132 / 53.18 / 1.232 | 2.5 / 13.5 / 4.854 | 2.635 / 13.31 / 4.925 | 5072 / 1.276 / 51.37 | v1x32 |
| LaneArray-N16 | 4096 | 16 | 52.19 / 1.8e+04 / 0.003642 | 2.077 / 61.05 / 1.074 | 2.489 / 7.529 / 8.705 | 3.033 / 6.945 / 9.436 | 5073 / 1.29 / 50.79 | v1x16 |
| LaneArray-N64 | 1 | 1024 | 3.105 / 1112 / 0.0009211 | 3.428 / 42.19 / 0.02427 | 3.394 / 40.83 / 0.02508 | 3.184 / 41.4 / 0.02473 | 4638 / 1.262 / 0.8112 | v2x1 |
| LaneArray-N64 | 16 | 1024 | 3.69 / 1.803e+04 / 0.0009088 | 3.503 / 61.51 / 0.2663 | 4.362 / 105 / 0.1561 | 4.253 / 115.6 / 0.1417 | 4643 / 1.935 / 8.467 | v1x16 |
| LaneArray-N64 | 64 | 256 | 6.498 / 1.79e+04 / 0.0009151 | 3.332 / 55.58 / 0.2948 | 3.693 / 14.92 / 1.098 | 4.884 / 62.23 / 0.2633 | 4644 / 1.438 / 11.39 | v1x32 |
| LaneArray-N64 | 256 | 64 | 16.06 / 1.784e+04 / 0.0009185 | 3.296 / 59.25 / 0.2765 | 3.647 / 18.11 / 0.9045 | 4.012 / 22.02 / 0.744 | 4644 / 1.485 / 11.03 | v1x32 |
| LaneArray-N64 | 1024 | 16 | 54.85 / 1.805e+04 / 0.0009079 | 3.237 / 64.19 / 0.2553 | 3.851 / 17.32 / 0.946 | 3.993 / 11.51 / 1.423 | 4644 / 1.385 / 11.83 | v1x16 |
| LaneArray-N64 | 4096 | 16 | 206.2 / 7.252e+04 / 0.0009037 | 3.562 / 245.2 / 0.2673 | 4.606 / 29.61 / 2.213 | 4.464 / 25.17 / 2.604 | 4647 / 5.47 / 11.98 | v1x32 |
| SyncFifo-D8 | 1 | 1024 | 1.69 / 8.5 / 0.1205 | 1.596 / 0.6777 / 1.511 | 1.466 / 0.6843 / 1.497 | 1.561 / 0.6804 / 1.505 | 5728 / 0.07594 / 13.48 | v1x1 |
| SyncFifo-D8 | 16 | 1024 | 1.661 / 139.4 / 0.1175 | 1.519 / 1.135 / 14.43 | 1.689 / 2.257 / 7.258 | 2.019 / 2.969 / 5.519 | 5729 / 0.282 / 58.09 | v1x16 |
| SyncFifo-D8 | 64 | 1024 | 2.752 / 550.2 / 0.1191 | 1.467 / 2.603 / 25.17 | 1.667 / 3.302 / 19.85 | 1.966 / 3.659 / 17.91 | 5729 / 0.5777 / 113.4 | v1x16 |
| SyncFifo-D8 | 256 | 1024 | 1.612 / 3103 / 0.08449 | 2.369 / 14.12 / 18.56 | 5.117 / 9.64 / 27.19 | 19.06 / 10.29 / 25.47 | 5733 / 7.338 / 35.72 | v1x32 |
| SyncFifo-D8 | 1024 | 1024 | 2.293 / 8934 / 0.1174 | 2.065 / 39.54 / 26.52 | 2.117 / 6.779 / 154.7 | 2.184 / 4.468 / 234.7 | 5729 / 6.165 / 170.1 | v1x32 |
| SyncFifo-D8 | 4096 | 256 | 5.035 / 8945 / 0.1172 | 2.419 / 39.01 / 26.88 | 2.617 / 4.124 / 254.3 | 3.61 / 5.12 / 204.8 | 5730 / 6.98 / 150.2 | v1x32 |
| SyncFifo-D64 | 1 | 1024 | 1.566 / 8.615 / 0.1189 | 1.371 / 0.6877 / 1.489 | 1.327 / 0.694 / 1.475 | 1.369 / 0.6909 / 1.482 | 5225 / 0.07514 / 13.63 | v1x1 |
| SyncFifo-D64 | 16 | 1024 | 1.681 / 174 / 0.09416 | 5.531 / 7.542 / 2.172 | 71.8 / 8.83 / 1.856 | 5.856 / 8.527 / 1.921 | 5225 / 1.707 / 9.598 | v1x1 |
| SyncFifo-D64 | 64 | 1024 | 2.667 / 563.5 / 0.1163 | 1.573 / 2.172 / 30.18 | 1.653 / 2.882 / 22.74 | 2.137 / 4.182 / 15.67 | 5225 / 0.5493 / 119.3 | v1x16 |
| SyncFifo-D64 | 256 | 1024 | 2.042 / 2152 / 0.1218 | 1.702 / 10.86 / 24.13 | 1.953 / 5.882 / 44.57 | 1.938 / 6.032 / 43.46 | 5225 / 2.132 / 122.9 | v1x16 |
| SyncFifo-D64 | 1024 | 1024 | 3.228 / 9333 / 0.1124 | 1.943 / 37.49 / 27.97 | 2.33 / 4.416 / 237.4 | 2.492 / 4.24 / 247.3 | 5226 / 6.926 / 151.4 | v1x32 |
| SyncFifo-D64 | 4096 | 256 | 9.5 / 9162 / 0.1145 | 3.083 / 37.98 / 27.61 | 3.575 / 4.221 / 248.4 | 3.838 / 4.001 / 262.1 | 5226 / 6.737 / 155.6 | v1x32 |
| SimpleDualPortRam | 1 | 1024 | 2.022 / 1.59 / 0.6439 | 2.222 / 0.1082 / 9.461 | 1.415 / 0.1194 / 8.58 | 1.861 / 0.1001 / 10.22 | 4666 / 0.0717 / 14.28 | v1x1 |
| SimpleDualPortRam | 16 | 1024 | 1.685 / 21.23 / 0.7719 | 1.487 / 0.2195 / 74.66 | 2.05 / 0.5592 / 29.3 | 2.217 / 0.5286 / 30.99 | 4667 / 0.3102 / 52.82 | v1x16 |
| SimpleDualPortRam | 64 | 1024 | 1.523 / 75.12 / 0.8725 | 1.301 / 0.447 / 146.6 | 1.498 / 0.3346 / 195.9 | 1.763 / 0.6014 / 109 | 4667 / 0.5018 / 130.6 | v1x16 |
| SimpleDualPortRam | 256 | 1024 | 1.641 / 290 / 0.9039 | 1.343 / 1.767 / 148.3 | 1.592 / 0.6195 / 423.2 | 1.816 / 0.636 / 412.2 | 4667 / 1.484 / 176.6 | v1x32 |
| SimpleDualPortRam | 1024 | 1024 | 1.881 / 1257 / 0.834 | 1.521 / 7.47 / 140.4 | 1.756 / 0.9501 / 1104 | 1.993 / 1.107 / 947.3 | 4667 / 6.426 / 163.2 | v1x32 |
| SimpleDualPortRam | 4096 | 256 | 4.435 / 1192 / 0.8797 | 1.698 / 7.566 / 138.6 | 2.556 / 1.139 / 920.4 | 3.374 / 1.083 / 968.6 | 4667 / 6.181 / 169.7 | v1x32 |
| RoundRobinArbiter4 | 1 | 1024 | 1.344 / 13.18 / 0.07772 | 1.198 / 1.111 / 0.9215 | 1.138 / 1.113 / 0.9199 | 1.132 / 1.095 / 0.9349 | 4604 / 0.06878 / 14.89 | v1x1 |
| RoundRobinArbiter4 | 16 | 1024 | 1.189 / 212.2 / 0.07721 | 1.121 / 1.337 / 12.26 | 1.549 / 2.29 / 7.156 | 1.56 / 1.747 / 9.38 | 4604 / 0.1414 / 115.9 | v1x16 |
| RoundRobinArbiter4 | 64 | 1024 | 1.226 / 832 / 0.07877 | 1.269 / 4.031 / 16.26 | 1.411 / 4.049 / 16.19 | 1.61 / 4.846 / 13.52 | 4605 / 0.4655 / 140.8 | v1x32 |
| RoundRobinArbiter4 | 256 | 1024 | 1.366 / 3322 / 0.07891 | 1.236 / 13.18 / 19.88 | 1.485 / 7.208 / 36.37 | 1.89 / 5.631 / 46.55 | 4605 / 1.56 / 168.1 | v1x32 |
| RoundRobinArbiter4 | 1024 | 1024 | 1.657 / 1.322e+04 / 0.07932 | 1.347 / 53.21 / 19.71 | 1.607 / 5.64 / 185.9 | 1.885 / 5.475 / 191.5 | 4604 / 5.178 / 202.5 | v1x16 |
| RoundRobinArbiter4 | 4096 | 256 | 3.149 / 1.371e+04 / 0.07648 | 1.669 / 56.58 / 18.53 | 2.463 / 6.129 / 171.1 | 2.375 / 5.127 / 204.5 | 4605 / 4.634 / 226.3 | v1x32 |

## Cold builds

SV emission, harness generation and compilation elapsed in seconds. Each fresh build served all six batch sizes for its design and internal thread count.

| Design | `--threads 1` | `--threads 2` |
|---|---:|---:|
| LaneArray-N1 | 5.137 | 5.204 |
| LaneArray-N4 | 5.181 | 5.035 |
| LaneArray-N16 | 5.071 | 4.932 |
| LaneArray-N64 | 4.643 | 4.638 |
| SyncFifo-D8 | 5.728 | 5.720 |
| SyncFifo-D64 | 5.225 | 5.800 |
| SimpleDualPortRam | 4.666 | 4.809 |
| RoundRobinArbiter4 | 4.604 | 4.616 |

## First failure

The Fault fixture contains a clocked `assert(not inject)`. Only trajectory 3073 drives `inject` high at one-based edge 500. Every engine and thread configuration returned **(3073, 500)** among 4,096 trajectories. The C++ harness checks `VerilatedContext::gotError()` following the emitted assertion. The driver measured cold wall time continuously from launching the Rust process, or starting fresh SV emission and Verilator compilation, through receipt of the failure result. Each configuration ran once. The build and run columns show the components of that interval.

| Engine | Fresh build (s) | Run process wall (s) | Cold wall to failure (s) |
|---|---:|---:|---:|
| scalar1 | 0 | 1.991 | 1.991 |
| soa1 | 0 | 0.01033 | 0.01033 |
| soa16 | 0 | 0.003563 | 0.003563 |
| soa32 | 0 | 0.003082 | 0.003082 |
| v1x1 | 4.423 | 0.1115 | 4.534 |
| v1x16 | 4.435 | 0.009236 | 4.444 |
| v1x32 | 4.417 | 0.01169 | 4.429 |
| v2x1 | 4.421 | 0.4155 | 4.837 |
| v2x8 | 4.411 | 0.1023 | 4.514 |
| v2x16 | 4.405 | 0.08988 | 4.495 |
