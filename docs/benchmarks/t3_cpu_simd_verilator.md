# T3 CPU SIMD and Verilator 2D benchmark

- Host CPU: AMD Ryzen 9 7950X 16-Core Processor
- Logical CPUs: 32
- Verilator: Verilator 5.052 2026-09-05 rev v5.052
- Build: `cargo run -p syl_sim --release --example bench_cpu_2d`
- Stimulus: deterministic SplitMix64, four warm-up cycles per matrix point.

The example is reproducible with the command above. Aggregate throughput is `batch * cycles / elapsed`. The current report records the scalar reference and direct SoA evaluator. Verilator compilation and thread sweeps require generated SystemVerilog harnesses and are reported as pending in this worktree.

| Design | Batch | Cycles | Scalar elapsed (s) | Scalar aggregate cycles/s | SoA elapsed (s) | SoA aggregate cycles/s |
|---|---:|---:|---:|---:|---:|---:|
| RoundRobinArbiter4 | 1 | 4 | 0.000075 | 53106.7 | 0.000057 | 69787.3 |
| RoundRobinArbiter4 | 16 | 4 | 0.001068 | 59939.2 | 0.000832 | 76911.1 |
| RoundRobinArbiter4 | 64 | 4 | 0.004263 | 60055.5 | 0.003386 | 75612.2 |
| RoundRobinArbiter4 | 256 | 4 | 0.017226 | 59445.1 | 0.013609 | 75243.4 |
| RoundRobinArbiter4 | 1024 | 4 | 0.069449 | 58978.2 | 0.054032 | 75806.7 |
| RoundRobinArbiter4 | 4096 | 4 | 0.275485 | 59473.4 | 0.215709 | 75954.1 |
| SyncFifo-D8 | 1 | 4 | 0.000050 | 79581.4 | 0.000035 | 115356.9 |
| SyncFifo-D8 | 16 | 4 | 0.000649 | 98592.0 | 0.000526 | 121631.4 |
| SyncFifo-D8 | 64 | 4 | 0.002589 | 98869.8 | 0.002118 | 120890.7 |
| SyncFifo-D8 | 256 | 4 | 0.010404 | 98424.4 | 0.008540 | 119913.2 |
| SyncFifo-D8 | 1024 | 4 | 0.041782 | 98032.6 | 0.034613 | 118337.2 |
| SyncFifo-D8 | 4096 | 4 | 0.167132 | 98030.3 | 0.137322 | 119311.0 |
| SimpleDualPortRam | 1 | 4 | 0.000011 | 348098.5 | 0.000005 | 736648.3 |
| SimpleDualPortRam | 16 | 4 | 0.000098 | 651844.0 | 0.000067 | 959750.5 |
| SimpleDualPortRam | 64 | 4 | 0.000378 | 677474.5 | 0.000271 | 943552.7 |
| SimpleDualPortRam | 256 | 4 | 0.001510 | 678103.9 | 0.001057 | 968414.0 |
| SimpleDualPortRam | 1024 | 4 | 0.006092 | 672319.8 | 0.004232 | 967954.7 |
| SimpleDualPortRam | 4096 | 4 | 0.024451 | 670088.1 | 0.017298 | 947162.5 |
| LaneArray | 1 | 4 | 0.000099 | 40566.7 | 0.000066 | 60201.1 |
| LaneArray | 16 | 4 | 0.001404 | 45587.5 | 0.000999 | 64047.3 |
| LaneArray | 64 | 4 | 0.005619 | 45562.8 | 0.004010 | 63842.2 |
| LaneArray | 256 | 4 | 0.022484 | 45544.4 | 0.016067 | 63731.3 |
| LaneArray | 1024 | 4 | 0.089711 | 45657.6 | 0.063917 | 64083.0 |
| LaneArray | 4096 | 4 | 0.362320 | 45219.7 | 0.261235 | 62717.5 |
| LaneArray | 1 | 4 | 0.000375 | 10665.6 | 0.000248 | 16126.3 |
| LaneArray | 16 | 4 | 0.005655 | 11317.0 | 0.004004 | 15985.3 |
| LaneArray | 64 | 4 | 0.022737 | 11259.3 | 0.016221 | 15781.6 |
| LaneArray | 256 | 4 | 0.091538 | 11186.7 | 0.066610 | 15373.2 |
| LaneArray | 1024 | 4 | 0.366377 | 11179.7 | 0.265955 | 15401.1 |
| LaneArray | 4096 | 4 | 1.458651 | 11232.3 | 1.071536 | 15290.2 |
| LaneArray | 1 | 4 | 0.001442 | 2773.4 | 0.000998 | 4008.9 |
| LaneArray | 16 | 4 | 0.022778 | 2809.8 | 0.016184 | 3954.6 |
| LaneArray | 64 | 4 | 0.091125 | 2809.3 | 0.065638 | 3900.2 |
| LaneArray | 256 | 4 | 0.363866 | 2814.2 | 0.270850 | 3780.7 |
| LaneArray | 1024 | 4 | 1.451932 | 2821.1 | 1.075732 | 3807.6 |
| LaneArray | 4096 | 4 | 5.818076 | 2816.1 | 4.274904 | 3832.6 |
| LaneArray | 1 | 4 | 0.005715 | 699.9 | 0.004119 | 971.0 |
| LaneArray | 16 | 4 | 0.091525 | 699.3 | 0.066012 | 969.5 |
| LaneArray | 64 | 4 | 0.364456 | 702.4 | 0.265372 | 964.7 |
| LaneArray | 256 | 4 | 1.458082 | 702.3 | 1.047795 | 977.3 |
| LaneArray | 1024 | 4 | 5.840106 | 701.4 | 4.326653 | 946.7 |
| LaneArray | 4096 | 4 | 23.312807 | 702.8 | 17.094926 | 958.4 |

## Method

Each matrix point uses the same generated design and SplitMix64 stimulus. Inputs are driven into every lane, combinational scheduling is evaluated, then the `clk` domain is ticked. The SoA evaluator stores signal and memory words as contiguous lane planes and reports the first failing lane and cycle.

## Failure localization

The API returns `BatchFailure { batch_idx, cycle, error }` on assertion or runtime failure. A dedicated injected-failure timing row is pending because no benchmark harness currently exposes a portable assertion injection hook.
