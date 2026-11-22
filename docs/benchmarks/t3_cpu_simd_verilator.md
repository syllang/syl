# T3 CPU baselines on Ryzen 9 7950X

At B=4096, the fastest measured Verilator configuration ran LaneArray-N64 at 11.54 million DUT cycles/s. The fastest SoA configuration on that design reached 2.37 million. On SimpleDualPortRam, SoA reached 1576 million and Verilator reached 362 million. One cycle advances one complete design instance, including all N internal lanes.

At B=4096, Verilator led on LaneArray-N4, LaneArray-N16, LaneArray-N64, RoundRobinArbiter4. SoA led on LaneArray-N1, SyncFifo-D8, SyncFifo-D64, SimpleDualPortRam.

## Reproduce

```sh
python3 crates/syl_sim/examples/bench_cpu_2d/run.py
cargo test -p syl_sim --test batch_cpu_test
```

The November 22, 2026 run used one AMD Ryzen 9 7950X socket, 16 physical cores and 32 logical CPUs, Linux 7.2.6, rustc 1.96.0 (LLVM 22.1.2), GCC 16.2.1 and Verilator 5.052. Rust used release mode with `-Ctarget-cpu=native`. The C++ harness used `-std=c++20 -O3 -march=native`. Disassembly of `syl_sim::fast_cpu::kernel::execute` in the release example shows eight-wide `vpaddq` and `vpandq` instructions using ZMM registers.

The T2 reference, SoA compiler and SV emitter consume the same specialized design inputs. SoA compiles the scheduled combinational logic and rising-edge updates into flat instruction arrays. Each tile stores up to 64 independent trajectories per word plane. Native workers own disjoint tiles and synchronize once at startup. The integration test counts heap allocations on each executing thread during stimulus, combinational evaluation and ticking. It recorded zero in the measured regions for all seven specializations tested.

Each throughput design uses two fresh Verilator builds with `--cc --exe --build -j 16 -O3 --assert`, `OPT_FAST=-O3` and `--threads 1` or `2`. We tested one host worker and up to 16 and 32 total threads across independent model instances. In the table, `vIxW` denotes I internal threads per model and W host workers. The Verilator column selects the highest measured median throughput for each point. The depth-64 FIFO emits a width-extension diagnostic comparing its seven-bit count with the constant parameter DEPTH. The build retains that diagnostic. [Complete samples and host identification](t3_cpu_2d.json)

## Measurements

Verilator evaluates twice per cycle. Each model settles low with its new inputs, then evaluates the rising edge. A final low settle precedes the output digest. All 48 output digests match the archived three-evaluation run at commit `d69cd08`. For SimpleDualPortRam at B=4096 and 256 cycles, the v1x32 median changed from 6.181 ms to 2.894 ms in the separate full runs. Its digest remained `6607019259109491407`.

All engines use counter-based SplitMix64 keyed by cycle, trajectory index, port and limb, including the high limbs of wide buses. The first edge asserts reset where present. Every configuration produced the same final digest over all output words and trajectories at each of the 48 points. The integration test separately compares all output bits against T2 before and after every one of 32 random edges on 17 trajectories, with one and four requested workers.

The Value-based batch API shares T2's event, expression and four-state semantics. Its regression tests cover simultaneous clock groups, a falling-only event, direct clock drives, assertion cycle numbers, uninitialized RAM and high impedance. The FastOp arithmetic tests include signed division, remainder, ordering, mixed signedness and 64-bit overflow. The concrete T2 IR provides a left-shift opcode. The negative arithmetic right-shift case is expressed in both engines as a repeated sign bit followed by the upper data bits.

Each cell gives **cold setup ms / hot elapsed ms / aggregate million cycles per second**. Rust cold setup includes source elaboration, specialization and batch allocation, plus instruction compilation for SoA. Verilator cold setup includes emission, model build and process-local setup. The bench executable itself was prebuilt. For B=1 the third number gives the single-trajectory rate. For larger B, aggregate throughput is B times the per-trajectory mean progress.

Hot time covers stimulus, clock edges and the final digest. SoA measures from the earliest tile start after the worker barrier to the last tile completion. Verilator measures between the model-ready and digest-complete barriers. The scalar timer surrounds the corresponding loop and digest. Worker creation, join and model destruction fall outside hot timing. The JSON also records process wall time. Each sub-250 ms first observation triggered two more fresh-process samples and the table uses the median. Longer observations were measured once.

Small points use 1,024 cycles per trajectory. Large LaneArray cases use 16 to 256 cycles to bound the serial reference runtime, so those rows describe short traces at the stated B. There was no excluded cycle warm-up or CPU pinning. OS and compiler caches stayed enabled and other jobs were active on the workstation. Sub-millisecond differences and small rankings should be read alongside the raw samples.

| Design | B | Cycles | Scalar Simulator | SoA 1T | SoA 16T | SoA 32T | Verilator 5.052 | Config |
|---|---:|---:|---:|---:|---:|---:|---:|---|
| LaneArray-N1 | 1 | 1024 | 1.577 / 18.14 / 0.05645 | 1.586 / 0.7528 / 1.36 | 1.456 / 0.7335 / 1.396 | 1.458 / 0.7479 / 1.369 | 4581 / 0.05814 / 17.61 | v1x1 |
| LaneArray-N1 | 16 | 1024 | 1.464 / 284.1 / 0.05767 | 1.321 / 1.345 / 12.18 | 1.711 / 1.774 / 9.235 | 1.806 / 1.716 / 9.549 | 4581 / 0.113 / 145.1 | v1x16 |
| LaneArray-N1 | 64 | 1024 | 1.605 / 1138 / 0.0576 | 1.564 / 3.812 / 17.19 | 1.792 / 1.581 / 41.46 | 1.953 / 4.086 / 16.04 | 4581 / 0.3624 / 180.9 | v1x32 |
| LaneArray-N1 | 256 | 1024 | 1.828 / 4560 / 0.05748 | 1.706 / 12.54 / 20.9 | 1.79 / 3.768 / 69.57 | 2.083 / 5.879 / 44.59 | 4581 / 1.296 / 202.3 | v1x32 |
| LaneArray-N1 | 1024 | 1024 | 2.666 / 1.824e+04 / 0.05748 | 1.652 / 46.57 / 22.52 | 1.989 / 5.384 / 194.8 | 2.736 / 4.538 / 231.1 | 4581 / 3.659 / 286.6 | v1x32 |
| LaneArray-N1 | 4096 | 256 | 6.197 / 1.816e+04 / 0.05774 | 2.122 / 44.54 / 23.54 | 2.469 / 4.531 / 231.4 | 2.979 / 3.438 / 305 | 4582 / 3.854 / 272.1 | v1x32 |
| LaneArray-N4 | 1 | 1024 | 1.516 / 71.53 / 0.01432 | 1.557 / 2.998 / 0.3415 | 1.551 / 3.009 / 0.3404 | 1.572 / 3.005 / 0.3408 | 4438 / 0.09694 / 10.56 | v1x1 |
| LaneArray-N4 | 16 | 1024 | 1.727 / 1140 / 0.01438 | 1.556 / 5.113 / 3.205 | 1.948 / 10.03 / 1.633 | 1.861 / 7.723 / 2.121 | 4438 / 0.1743 / 94.02 | v1x16 |
| LaneArray-N4 | 64 | 1024 | 1.891 / 4549 / 0.01441 | 1.716 / 17.32 / 3.783 | 2.056 / 4.085 / 16.04 | 2.111 / 16.12 / 4.066 | 4438 / 0.5215 / 125.7 | v1x32 |
| LaneArray-N4 | 256 | 1024 | 2.658 / 1.824e+04 / 0.01437 | 1.737 / 46.48 / 5.64 | 1.996 / 25.76 / 10.18 | 2.244 / 28.74 / 9.121 | 4438 / 2.023 / 129.6 | v1x32 |
| LaneArray-N4 | 1024 | 256 | 5.117 / 1.833e+04 / 0.0143 | 1.585 / 50.85 / 5.155 | 2.263 / 10.74 / 24.42 | 2.944 / 10.98 / 23.88 | 4438 / 1.497 / 175.1 | v1x32 |
| LaneArray-N4 | 4096 | 64 | 16.56 / 1.835e+04 / 0.01429 | 1.712 / 50.85 / 5.156 | 1.957 / 5.78 / 45.36 | 2.367 / 4.921 / 53.27 | 4438 / 1.935 / 135.5 | v1x16 |
| LaneArray-N16 | 1 | 1024 | 1.897 / 287.3 / 0.003564 | 2.035 / 11.36 / 0.09013 | 1.958 / 11.86 / 0.08632 | 1.841 / 11.4 / 0.08986 | 4461 / 0.2764 / 3.705 | v1x1 |
| LaneArray-N16 | 16 | 1024 | 1.961 / 4630 / 0.003538 | 1.96 / 15.89 / 1.031 | 2.586 / 30.11 / 0.5441 | 2.347 / 30.49 / 0.5373 | 4461 / 0.4245 / 38.59 | v1x16 |
| LaneArray-N16 | 64 | 1024 | 2.868 / 1.892e+04 / 0.003464 | 1.887 / 71.9 / 0.9114 | 2.676 / 17.58 / 3.728 | 2.733 / 64.85 / 1.011 | 4461 / 1.468 / 44.63 | v1x32 |
| LaneArray-N16 | 256 | 256 | 5.354 / 1.827e+04 / 0.003586 | 2.005 / 52.87 / 1.24 | 2.654 / 21.25 / 3.083 | 3.283 / 30.9 / 2.121 | 4461 / 1.264 / 51.83 | v1x32 |
| LaneArray-N16 | 1024 | 64 | 14.89 / 1.89e+04 / 0.003468 | 1.947 / 51.87 / 1.264 | 2.427 / 14.53 / 4.51 | 2.838 / 11.92 / 5.497 | 4461 / 0.9244 / 70.9 | v1x32 |
| LaneArray-N16 | 4096 | 16 | 53.36 / 1.809e+04 / 0.003622 | 2.027 / 61.8 / 1.06 | 2.398 / 6.744 / 9.718 | 2.812 / 6.912 / 9.482 | 4463 / 1.662 / 39.44 | v1x32 |
| LaneArray-N64 | 1 | 1024 | 3.109 / 1139 / 0.0008994 | 3.906 / 43.35 / 0.02362 | 3.168 / 46.92 / 0.02183 | 3.633 / 43.11 / 0.02375 | 4642 / 1.228 / 0.8337 | v1x1 |
| LaneArray-N64 | 16 | 1024 | 3.798 / 1.817e+04 / 0.0009015 | 3.316 / 86.05 / 0.1904 | 4.161 / 110.1 / 0.1488 | 3.973 / 122.2 / 0.1341 | 4642 / 1.711 / 9.574 | v1x16 |
| LaneArray-N64 | 64 | 256 | 6.31 / 1.819e+04 / 0.0009005 | 3.365 / 70.93 / 0.231 | 3.796 / 14.96 / 1.095 | 5.114 / 68.57 / 0.2389 | 4642 / 1.393 / 11.77 | v1x32 |
| LaneArray-N64 | 256 | 64 | 15.78 / 1.832e+04 / 0.0008945 | 3.65 / 62.37 / 0.2627 | 5.209 / 17.45 / 0.9389 | 4.019 / 21.43 / 0.7645 | 4642 / 1.421 / 11.53 | v1x32 |
| LaneArray-N64 | 1024 | 16 | 54.37 / 1.842e+04 / 0.0008893 | 3.388 / 61.18 / 0.2678 | 3.895 / 15.25 / 1.074 | 4.546 / 14.95 / 1.096 | 4642 / 1.412 / 11.6 | v1x16 |
| LaneArray-N64 | 4096 | 16 | 201.6 / 7.269e+04 / 0.0009016 | 3.683 / 242.4 / 0.2704 | 4.292 / 30.53 / 2.146 | 4.28 / 27.63 / 2.372 | 4645 / 5.677 / 11.54 | v1x32 |
| SyncFifo-D8 | 1 | 1024 | 1.496 / 9.061 / 0.113 | 1.361 / 0.6758 / 1.515 | 1.218 / 0.6745 / 1.518 | 1.265 / 0.6681 / 1.533 | 4508 / 0.05575 / 18.37 | v1x1 |
| SyncFifo-D8 | 16 | 1024 | 1.247 / 143.1 / 0.1145 | 1.334 / 0.8246 / 19.87 | 1.632 / 1.407 / 11.64 | 1.63 / 2.278 / 7.192 | 4509 / 0.1097 / 149.4 | v1x16 |
| SyncFifo-D8 | 64 | 1024 | 1.302 / 569.3 / 0.1151 | 1.498 / 2.483 / 26.4 | 1.705 / 2.228 / 29.41 | 2.089 / 3.155 / 20.77 | 4509 / 0.2889 / 226.8 | v1x32 |
| SyncFifo-D8 | 256 | 1024 | 1.742 / 2275 / 0.1152 | 1.819 / 9.864 / 26.57 | 1.686 / 3.783 / 69.29 | 2.036 / 3.313 / 79.12 | 4509 / 1.27 / 206.4 | v1x32 |
| SyncFifo-D8 | 1024 | 1024 | 2.159 / 9134 / 0.1148 | 1.58 / 38.07 / 27.54 | 2.07 / 4.127 / 254.1 | 2.158 / 3.733 / 280.9 | 4509 / 4.183 / 250.7 | v1x32 |
| SyncFifo-D8 | 4096 | 256 | 4.633 / 9290 / 0.1129 | 2.095 / 37.8 / 27.74 | 2.265 / 3.949 / 265.5 | 3.281 / 3.504 / 299.2 | 4509 / 5.087 / 206.1 | v1x32 |
| SyncFifo-D64 | 1 | 1024 | 1.466 / 9.04 / 0.1133 | 1.27 / 0.6653 / 1.539 | 1.532 / 0.6796 / 1.507 | 1.447 / 0.6729 / 1.522 | 4951 / 0.05499 / 18.62 | v1x1 |
| SyncFifo-D64 | 16 | 1024 | 1.364 / 143.6 / 0.1141 | 1.469 / 1.08 / 15.17 | 1.623 / 2.443 / 6.706 | 1.642 / 1.608 / 10.19 | 4951 / 0.1069 / 153.3 | v1x16 |
| SyncFifo-D64 | 64 | 1024 | 1.454 / 568.9 / 0.1152 | 1.563 / 2.182 / 30.03 | 1.679 / 2.982 / 21.98 | 1.909 / 4.113 / 15.93 | 4951 / 0.3267 / 200.6 | v1x32 |
| SyncFifo-D64 | 256 | 1024 | 1.806 / 2306 / 0.1137 | 1.604 / 9.984 / 26.26 | 1.877 / 5.164 / 50.77 | 1.937 / 4.8 / 54.61 | 4951 / 1.256 / 208.7 | v1x32 |
| SyncFifo-D64 | 1024 | 1024 | 3.111 / 9146 / 0.1146 | 1.706 / 37.95 / 27.63 | 2.245 / 4.402 / 238.2 | 2.328 / 3.331 / 314.8 | 4951 / 3.907 / 268.4 | v1x32 |
| SyncFifo-D64 | 4096 | 256 | 8.529 / 9301 / 0.1127 | 3.474 / 37.2 / 28.18 | 3.198 / 3.982 / 263.4 | 3.469 / 3.942 / 266 | 4952 / 4.336 / 241.8 | v1x32 |
| SimpleDualPortRam | 1 | 1024 | 1.227 / 1.244 / 0.823 | 1.015 / 0.08542 / 11.99 | 1.145 / 0.08549 / 11.98 | 1.16 / 0.08161 / 12.55 | 4412 / 0.04611 / 22.21 | v1x1 |
| SimpleDualPortRam | 16 | 1024 | 1.352 / 19.88 / 0.8243 | 1.141 / 0.1515 / 108.1 | 1.415 / 0.3332 / 49.17 | 1.488 / 0.3246 / 50.48 | 4412 / 0.09666 / 169.5 | v1x16 |
| SimpleDualPortRam | 64 | 1024 | 1.22 / 81.17 / 0.8074 | 1.171 / 0.4372 / 149.9 | 1.546 / 0.3095 / 211.7 | 1.712 / 0.4503 / 145.5 | 4413 / 0.314 / 208.7 | v1x32 |
| SimpleDualPortRam | 256 | 1024 | 1.435 / 320.6 / 0.8176 | 1.319 / 1.708 / 153.4 | 1.554 / 0.4352 / 602.3 | 1.85 / 0.5532 / 473.9 | 4413 / 1.071 / 244.8 | v1x32 |
| SimpleDualPortRam | 1024 | 1024 | 1.868 / 1276 / 0.8215 | 1.304 / 7.076 / 148.2 | 1.85 / 0.857 / 1224 | 1.841 / 0.7929 / 1322 | 4413 / 3.577 / 293.1 | v1x32 |
| SimpleDualPortRam | 4096 | 256 | 4.341 / 1291 / 0.812 | 1.565 / 7.408 / 141.5 | 1.759 / 0.6831 / 1535 | 2.049 / 0.6653 / 1576 | 4413 / 2.894 / 362.3 | v1x32 |
| RoundRobinArbiter4 | 1 | 1024 | 1.119 / 13.38 / 0.07652 | 1.104 / 1.178 / 0.8693 | 1.168 / 1.18 / 0.8681 | 1.386 / 1.171 / 0.8746 | 4421 / 0.05074 / 20.18 | v1x1 |
| RoundRobinArbiter4 | 16 | 1024 | 1.282 / 213.9 / 0.07659 | 1.168 / 1.684 / 9.73 | 1.553 / 1.816 / 9.024 | 1.413 / 2.188 / 7.489 | 4422 / 0.09845 / 166.4 | v1x16 |
| RoundRobinArbiter4 | 64 | 1024 | 1.057 / 874.9 / 0.07491 | 1.101 / 4.438 / 14.77 | 1.5 / 5.262 / 12.45 | 1.657 / 5.211 / 12.58 | 4422 / 0.2973 / 220.5 | v1x32 |
| RoundRobinArbiter4 | 256 | 1024 | 1.298 / 3418 / 0.07668 | 1.412 / 13.6 / 19.28 | 1.427 / 6.964 / 37.64 | 1.863 / 6.889 / 38.05 | 4422 / 1.068 / 245.4 | v1x32 |
| RoundRobinArbiter4 | 1024 | 1024 | 2.128 / 1.362e+04 / 0.07697 | 1.233 / 52.56 / 19.95 | 2.198 / 6.932 / 151.3 | 2.04 / 5.623 / 186.5 | 4422 / 3.794 / 276.4 | v1x16 |
| RoundRobinArbiter4 | 4096 | 256 | 2.935 / 1.419e+04 / 0.07392 | 1.63 / 55.43 / 18.92 | 2.415 / 5.899 / 177.8 | 2.299 / 5.298 / 197.9 | 4422 / 3.544 / 295.9 | v1x16 |

## Cold builds

SV emission, harness generation and compilation elapsed in seconds. Each fresh build served all six batch sizes for its design and internal thread count.

| Design | `--threads 1` | `--threads 2` |
|---|---:|---:|
| LaneArray-N1 | 4.581 | 4.445 |
| LaneArray-N4 | 4.438 | 4.441 |
| LaneArray-N16 | 4.461 | 4.455 |
| LaneArray-N64 | 4.642 | 4.625 |
| SyncFifo-D8 | 4.508 | 4.449 |
| SyncFifo-D64 | 4.951 | 4.544 |
| SimpleDualPortRam | 4.412 | 4.390 |
| RoundRobinArbiter4 | 4.421 | 4.405 |

## First failure

The Fault fixture contains a clocked `assert(not inject)`. Only trajectory 3073 drives `inject` high at one-based edge 500. Every engine and thread configuration returned **(3073, 500)** among 4,096 trajectories. The C++ harness checks `VerilatedContext::gotError()` following the emitted assertion. The driver measured cold wall time continuously from launching the Rust process, or starting fresh SV emission and Verilator compilation, through receipt of the failure result. Each configuration ran once. The build and run columns show the components of that interval.

At edge 499 the only stored state is register q, exposed as y. Rust captured q directly and the C++ harness captured y after the rising edge. All ten configurations returned the same 4,096 bits in trajectory order (2,077 ones) and digest `59144026378983870`. The Python driver reconstructs every expected q bit from the independent SplitMix64 input formula, checks the full bitmap and digest, and validates the edge-500 failure coordinate separately. The JSON stores each bitmap in `checkpoint_bits`.

| Engine | Fresh build (s) | Run process wall (s) | Cold wall to failure (s) |
|---|---:|---:|---:|
| scalar1 | 0 | 2.272 | 2.272 |
| soa1 | 0 | 0.01131 | 0.01131 |
| soa16 | 0 | 0.003141 | 0.003141 |
| soa32 | 0 | 0.003302 | 0.003302 |
| v1x1 | 4.413 | 0.0802 | 4.493 |
| v1x16 | 4.415 | 0.009637 | 4.425 |
| v1x32 | 4.427 | 0.008438 | 4.435 |
| v2x1 | 4.428 | 0.4636 | 4.892 |
| v2x8 | 4.391 | 0.1022 | 4.493 |
| v2x16 | 4.433 | 0.05582 | 4.488 |
