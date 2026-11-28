# 单轨迹仿真与 Verilator 5.052

在 Ryzen 9 7950X 上，Cranelift JIT 在 12 个设计的 100,000 拍热执行中均快于单线程 Verilator，吞吐比为 **1.50× 至 10.88×**。串行依赖最深的 DeepFsmChain 达到 6.87 Mcycles/s，Verilator 为 4.58 Mcycles/s。JIT 原生编译及计划准备耗时 0.42 至 41.64 ms，包含前端、启动进程和运行一拍的墙钟耗时为 3.68 至 47.14 ms。两档周期共 104 条测量记录的 64 位摘要均通过参考引擎校验。[原始数据](single_trace_b1.json)

## 热吞吐

单位为百万个完整 DUT 时钟周期每秒。每条轨迹在第一拍复位。Family 列使用单条轨迹内的成员作为 SIMD 通道。空白表示该设计没有符合条件的独立同构子树。

### 1,024 拍

| 设计 | CpuRef | 标量 CpuSimd | Family SIMD | CpuJit | Verilator v1x1 | JIT / Verilator |
|---|---:|---:|---:|---:|---:|---:|
| RoundRobinArbiter4 | 0.0625 | 2.77 | — | 151.41 | 21.71 | 6.97× |
| SimpleDualPortRam | 0.87 | 9.61 | — | 198.45 | 24.03 | 8.26× |
| SyncFifo-D8 | 0.11 | 3.29 | — | 135.38 | 20.46 | 6.62× |
| SyncFifo-D64 | 0.11 | 3.32 | — | 141.16 | 18.91 | 7.47× |
| LaneArray-N1 | 0.0549 | 3.26 | — | 189.24 | 19.22 | 9.85× |
| LaneArray-N4 | 0.0132 | 0.78 | 2.15 | 44.69 | 7.18 | 6.22× |
| LaneArray-N16 | 0.0027 | 0.28 | 1.04 | 16.60 | 3.83 | 4.34× |
| LaneArray-N64 | 0.0008 | 0.0759 | 0.41 | 3.64 | 0.83 | 4.40× |
| WideAluArray | 0.0296 | 1.92 | 4.05 | 69.15 | 21.14 | 3.27× |
| DeepFsmChain | 0.0025 | 0.31 | — | 6.82 | 3.85 | 1.77× |
| SharedRamArbiter | 0.0758 | 3.10 | — | 90.20 | 9.20 | 9.80× |
| FaultHuntPipeline | 0.0974 | 2.79 | — | 149.88 | 23.03 | 6.51× |

### 100,000 拍

| 设计 | CpuRef | 标量 CpuSimd | Family SIMD | CpuJit | Verilator v1x1 | JIT / Verilator |
|---|---:|---:|---:|---:|---:|---:|
| RoundRobinArbiter4 | 0.0683 | 3.39 | — | 163.94 | 21.89 | 7.49× |
| SimpleDualPortRam | 0.87 | 19.98 | — | 212.30 | 23.69 | 8.96× |
| SyncFifo-D8 | 0.11 | 4.29 | — | 155.50 | 20.29 | 7.66× |
| SyncFifo-D64 | 0.11 | 4.32 | — | 156.76 | 20.57 | 7.62× |
| LaneArray-N1 | 0.0539 | 4.23 | — | 210.44 | 19.33 | 10.88× |
| LaneArray-N4 | 0.0129 | 1.10 | 2.14 | 59.20 | 7.43 | 7.97× |
| LaneArray-N16 | 0.0033 | 0.29 | 1.06 | 16.87 | 3.82 | 4.42× |
| LaneArray-N64 | 0.0008 | 0.0671 | 0.34 | 3.69 | 0.84 | 4.42× |
| WideAluArray | 0.0336 | 2.22 | 3.99 | 102.71 | 20.10 | 5.11× |
| DeepFsmChain | 0.0024 | 0.32 | — | 6.87 | 4.58 | 1.50× |
| SharedRamArbiter | 0.0753 | 3.88 | — | 95.24 | 15.21 | 6.26× |
| FaultHuntPipeline | 0.0954 | 3.38 | — | 160.56 | 23.30 | 6.89× |

## 冷启动和热执行

下表使用毫秒。JIT 编译列从已具体化的设计开始，含字节码编译、活跃位分析、机器码生成与链接。首拍墙钟来自另起进程的 K=1 实验。Verilator 首拍包含生成 SystemVerilog、从空目录构建 C++ 模型与启动进程。已构建模型的热执行见上一节。

| 设计 | JIT 编译 | JIT 首拍墙钟 | JIT 热执行 1,024 拍 | JIT 热执行 100,000 拍 | Verilator 首拍墙钟 |
|---|---:|---:|---:|---:|---:|
| RoundRobinArbiter4 | 1.11 | 4.44 | 0.0068 | 0.610 | 4464.1 |
| SimpleDualPortRam | 0.52 | 3.68 | 0.0052 | 0.471 | 4909.7 |
| SyncFifo-D8 | 1.01 | 4.55 | 0.0076 | 0.643 | 4809.9 |
| SyncFifo-D64 | 0.88 | 4.82 | 0.0073 | 0.638 | 4570.1 |
| LaneArray-N1 | 0.79 | 4.75 | 0.0054 | 0.475 | 4672.4 |
| LaneArray-N4 | 3.72 | 6.35 | 0.0229 | 1.689 | 4553.4 |
| LaneArray-N16 | 10.04 | 15.26 | 0.0617 | 5.928 | 4463.1 |
| LaneArray-N64 | 41.64 | 47.14 | 0.2814 | 27.075 | 5080.8 |
| WideAluArray | 1.37 | 6.59 | 0.0148 | 0.974 | 4844.4 |
| DeepFsmChain | 22.07 | 26.88 | 0.1501 | 14.552 | 5024.5 |
| SharedRamArbiter | 1.28 | 4.80 | 0.0114 | 1.050 | 4903.1 |
| FaultHuntPipeline | 0.84 | 4.30 | 0.0068 | 0.623 | 5082.5 |

## 单轨迹路由

兼容快速两态语义的确定性任务在 B=1、K≥1,000 时自动选择 CpuJit。显式选择 cpu-simd 会尝试共享子树程序。K=100,000 时，Family 相对于标量 CpuSimd 在 LaneArray-N4/N16/N64 分别加速 1.94×、3.68×、5.06×，在 WideAluArray 加速 1.80×。本机 release 二进制的 family 执行器含有操作 YMM 行数据的 AVX2 指令，包括 `vpsubq`、`vpand` 与 `vpxor`。CPU 验证子树间的数据依赖和归一化后的字节码，父实例的寄存器、RAM 和断言可在同一上升沿采样子实例输出。DeepFsmChain 的后级消费前级同拍结果，使用串行组合求值。

在同机 RTX 2080 Ti 上额外测量了大族的 release CLI 墙钟。LaneArray-N256、K=1,024 的 CpuJit 为 277 ms，CUDA family 缓存命中为 743 ms。K=100,000 分别为 395 ms 和 2,166 ms。N384、K=1,024 连跑三轮，JIT 与缓存命中 CUDA 的中位数分别为 523 ms 和 589 ms，两个引擎的摘要相同。N448、N512 与 N1024 的 CUDA family 内核启动均报告每个 block 的资源需求超限。这些测点支持在本机对单轨迹继续使用 JIT。GPU 的多轨迹阈值沿用既有模型。

## 测量方法与复现

- Rust 1.96.0 release 使用 `-C target-cpu=native`。Verilator 5.052 使用 `--threads 1 --assert -O3`，C++ 使用 GCC 16.2.1、`-O3 -march=native`。各仿真进程固定于逻辑 CPU 4，主机沿用其频率调节设置。
- 所有引擎使用相同的 T3 SplitMix64 刺激，seed 和轨迹号均为零。输入包含宽总线的每一个 limb。Rust 热执行包含工作区初始化、刺激、采样、末拍低相位稳定与摘要归约。Verilator 在热计时开始前完成模型初始化。测试另外逐拍比对每个输出 limb，并覆盖首错拍号、轨迹号、断言编号及参考 VCD 回放。
- 当单时钟设计的组合逻辑不直接读取时钟引脚（`!clock_used_in_comb`）时，CpuJit 在热循环中生成当拍激励，随后执行一次经活跃位裁剪的高相位组合求值与边沿提交。中间拍省去低相位输出传播，在仿真结束或观测轨迹前缀输出时才单独计算可见的低相位输出。Verilator 热循环每拍调用两次 `eval()`，分别对应低相位与上升沿，循环结束后再调用一次低相位 `eval()`。逐拍验证的可见输出与全部基准摘要均逐位一致。热吞吐对比包含单时钟周期融合和死位消除所减少的计算量。
- 每个设计为 Verilator 建立一个新构建目录，编译墙钟在两档周期中复用。若首次热执行小于 250 ms，则使用三次独立进程的热耗时中位数，否则记录一次。JSON 保留各次热耗时、选中样本的冷耗时与热耗时、全程进程墙钟、冷加热耗时、首拍墙钟、吞吐和摘要。Verilator 的 `compile_ms` 包括从源文件到可执行模型的构建，Rust 的 `cold_ms` 包括前端及计划准备。
- JIT 通过 Cranelift 生成宿主机器码。组合中间量驻留于 SSA，活跃位分析删除不影响边沿或输出的计算。小位宽优先选择可编为单个立即数查找表。JIT 工作区可供同一设计的多次独立运行复用。

在仓库根目录运行以下命令。第二条命令仅在所有引擎通过摘要校验后写入 JSON。

```sh
export RUSTFLAGS='-C target-cpu=native'
cargo build --locked --release -p syl_sim --example bench_single_trace
python3 crates/syl_sim/examples/bench_single_trace/run.py docs/benchmarks/single_trace_b1.json --cpu 4
```
