# RTX 2080 Ti 上的二维 RTL 仿真

**64 个 lane、单条轨迹跑 1,000 拍，共享模板将 kernel 时间从 121.753 ms 降至 6.719 ms，加速 18.12×。** 同一设计的冷编译从 17.155 s 降至 2.311 s。到 B=16,384，Mode 1 和 Mode 2 的吞吐分别为 14.07 和 2.22 Mcycles/s。这里一拍指一个完整 DUT 的时钟周期。

## 实验条件与正确性

2026-11-22，NVIDIA GeForce RTX 2080 Ti，11,264 MiB，PCIe 3.0 ×16，驱动 615.71.09，nvcc 13.4.92，编译选项 `-O3 -arch=sm_75`。本表每个配置测一次。频率、温度与操作系统页缓存未锁定，交叉点仅能定位到相邻采样批量之间。

- **Mode 1** 每条线程执行一个完整 DUT。信号以 64 位字拆分，按 `state[word_slot * B + batch]` 排列。邻接 batch 读取同一槽位时地址连续。
- **Mode 2** 每个 batch 占一个 block，block 内恰有 F 条线程。生成器从 `instance_families()` 取得成员及其子树，检查各成员组合赋值、端口切片和边沿提交逻辑的同构性，再发射一份组合函数和一份时钟函数。线程通过 `family_slot_map[lane][local_slot]` 访问本 lane。第零线程生成公共输入，线程在公共输入与寄存器提交边界同步。LaneArray 的顶层输出只连接到外部观测端口，因此在每次观测前由第零线程打包。仅回传摘要时每次仿真打包一次，逐拍观测时每拍打包。两种模式使用相同的 SoA 物理布局，Mode 2 的同一 warp 会访问不同 lane 的槽位。
- **逐位验证** 使用 T2 `StateMode::TwoState`，初始寄存器与 RAM 清零，第 1 拍复位。对每个批次、拍号及输入字槽生成独立的 SplitMix64 随机字。正确性测试既比较连续执行的最终输出，也逐拍比较全部输出与 CPU 下降沿稳定后的端口值。原矩阵对应 23 拍差分。审查后的回归测试将深度 256 和 4096 的 RAM 延长至 256 拍，并要求每种深度出现非零读出。覆盖 B=7、LaneArray N=1/4/16/64、FIFO 深度 8/64、RAM 深度 16/256/4096 和仲裁器，每个设计均跑两种模式。两态逐位比较零差异。N=64 包含 2,624 位 payload 总线，跨字的每一段都参与比较。
- **设备端可观测性** 每个正常运行都将所有 batch 的所有末拍输出字写入设备缓冲区，独立 kernel 计算加权 64 位校验和并随断言摘要返回。LaneArray 的每个矩阵点均校验两种映射的摘要。标量标准库吞吐矩阵仅运行 Mode 1。正确性测试独立检查原始输出字。断言在寄存器和 RAM 提交前采样，单个 64 位 `atomicMin` 按拍号、batch、断言 ID 顺序选择首个失败。
- **支持范围** 此原型面向单个顶层时钟、同步复位的两态字级设计。共享模板路径支持独立同构子树中的寄存器、RAM 和时钟断言，以及直接输入切片、输出汇聚。成员拥有各自的 RAM 区间和全局断言编号。顶层时钟断言在成员提交前采样已汇聚的输出。标量标准库设计在 Mode 2 下以 F=1 验证。生成器对超出支持范围的结构返回诊断。

## 复现

```sh
cargo test -p syl_sim --test gpu_cuda_test -- --nocapture
cargo run --release -p syl_sim --example bench_gpu_2d -- /tmp/t3-failure.vcd > /tmp/t3-rtx.jsonl
```

Rust 构建不链接 CUDA。缺少 nvcc 时仍执行模板生成测试，只有编译器时可编译自包含 runner，运行时通过 `cudaGetDeviceCount` 判断设备是否可用。第二条命令写出 101 条 JSON 记录，每条轨迹使用 batch 序号作为可复现种子编号。下列矩阵来自实现提交 `db2770fec38111a2bfa9746cefc2ce3e1ab51a0b`。

## 编译与启动

每个设计及映射只构建一次可执行文件，并复用于不同 B。lower 从已完成 elaboration 的 IR 开始，gen 包含共享模板验证及完整 CUDA 源生成，cold 包含写源码和执行 nvcc 至链接完成。每个 hot 值都是直接执行已有二进制至进程退出的墙钟，包含 CUDA 上下文、分配、清零、kernel、传输和清理。单次冷使用成本可将 lower、gen、cold 和对应 hot 相加。

| 设计 | N 或深度 | Mode | lower ms | gen ms | cold ms |
|---|---:|---:|---:|---:|---:|
| LaneArray | 1 | 1 | 0.0584 | 0.0241 | 1985.45 |
| LaneArray | 1 | 2 | 0.0584 | 0.0627 | 1997.88 |
| LaneArray | 4 | 1 | 0.1575 | 0.0728 | 2136.27 |
| LaneArray | 4 | 2 | 0.1575 | 0.1591 | 2049.71 |
| LaneArray | 16 | 1 | 0.6259 | 0.3754 | 3479.93 |
| LaneArray | 16 | 2 | 0.6259 | 0.4920 | 2075.53 |
| LaneArray | 64 | 1 | 1.5334 | 0.8735 | 17155.22 |
| LaneArray | 64 | 2 | 1.5334 | 2.8664 | 2310.82 |
| SyncFifo | 8 | 1 | 0.0423 | 0.0260 | 2070.14 |
| SyncFifo | 64 | 1 | 0.0514 | 0.0352 | 2013.80 |
| SimpleDualPortRam | 16 | 1 | 0.0267 | 0.0158 | 1954.19 |
| SimpleDualPortRam | 256 | 1 | 0.0299 | 0.0172 | 2135.17 |
| SimpleDualPortRam | 4096 | 1 | 0.0291 | 0.0175 | 1972.73 |
| RoundRobinArbiter4 | 1 | 1 | 0.0386 | 0.0365 | 2030.26 |

## 稳态 kernel 与进程总账

H2D 为 16 字节摘要初始化。D2H 为 16 字节失败键及校验和。H2D、D2H、kernel 和归约分别由 CUDA events 计时。显存列为全部分配仍存活时两次 `cudaMemGetInfo` 的空闲显存差，包含模块与分配器开销，取整到 MiB。原始输出还给出设备全局占用量。每行 K=1,000，单轨迹列为一条轨迹在该批量下的完成速率，aggregate 为所有 B 条轨迹的总吞吐。标量设计的吞吐矩阵使用 Mode 1。

| 设计 | N 或深度 | Mode | B | kernel ms | 单轨迹 cycles/s | aggregate Mcycles/s | H2D ms | D2H ms | hot ms | 增量 MiB |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| LaneArray | 1 | 1 | 1 | 1.593 | 627749.5 | 0.628 | 0.01043 | 0.02304 | 323.10 | 4 |
| LaneArray | 1 | 1 | 16 | 1.331 | 751491.0 | 12.024 | 0.01078 | 0.01862 | 228.61 | 4 |
| LaneArray | 1 | 1 | 64 | 1.317 | 759546.0 | 48.611 | 0.00986 | 0.01901 | 230.21 | 4 |
| LaneArray | 1 | 1 | 256 | 1.366 | 731918.7 | 187.371 | 0.01110 | 0.01888 | 220.59 | 4 |
| LaneArray | 1 | 1 | 1024 | 1.351 | 740398.5 | 758.168 | 0.00867 | 0.01872 | 212.21 | 4 |
| LaneArray | 1 | 1 | 4096 | 1.332 | 750967.2 | 3075.962 | 0.00864 | 0.01885 | 245.99 | 4 |
| LaneArray | 1 | 1 | 16384 | 2.970 | 336745.7 | 5517.241 | 0.00973 | 0.02035 | 198.78 | 10 |
| LaneArray | 1 | 2 | 1 | 2.155 | 464042.3 | 0.464 | 0.01078 | 0.02003 | 314.99 | 4 |
| LaneArray | 1 | 2 | 16 | 1.462 | 684017.0 | 10.944 | 0.00947 | 0.02010 | 241.98 | 4 |
| LaneArray | 1 | 2 | 64 | 1.321 | 757208.6 | 48.461 | 0.00864 | 0.01872 | 228.44 | 4 |
| LaneArray | 1 | 2 | 256 | 1.219 | 820640.8 | 210.084 | 0.00928 | 0.01936 | 223.56 | 4 |
| LaneArray | 1 | 2 | 1024 | 3.503 | 285505.5 | 292.358 | 0.01053 | 0.01984 | 223.59 | 4 |
| LaneArray | 1 | 2 | 4096 | 8.561 | 116814.6 | 478.472 | 0.00941 | 0.01974 | 211.37 | 4 |
| LaneArray | 1 | 2 | 16384 | 31.478 | 31768.5 | 520.494 | 0.00973 | 0.01853 | 243.98 | 10 |
| LaneArray | 4 | 1 | 1 | 5.906 | 169310.6 | 0.169 | 0.01142 | 0.02022 | 325.62 | 4 |
| LaneArray | 4 | 1 | 16 | 5.218 | 191633.1 | 3.066 | 0.00925 | 0.01910 | 252.99 | 4 |
| LaneArray | 4 | 1 | 64 | 5.589 | 178923.1 | 11.451 | 0.00960 | 0.01888 | 218.85 | 4 |
| LaneArray | 4 | 1 | 256 | 16.880 | 59241.1 | 15.166 | 0.00909 | 0.02794 | 240.06 | 4 |
| LaneArray | 4 | 1 | 1024 | 15.723 | 63600.7 | 65.127 | 0.00886 | 0.02074 | 248.95 | 4 |
| LaneArray | 4 | 1 | 4096 | 15.469 | 64647.3 | 264.795 | 0.00931 | 0.02426 | 259.99 | 8 |
| LaneArray | 4 | 1 | 16384 | 62.796 | 15924.6 | 260.909 | 0.00867 | 0.02394 | 278.24 | 20 |
| LaneArray | 4 | 2 | 1 | 1.852 | 539966.1 | 0.540 | 0.01024 | 0.02006 | 282.95 | 4 |
| LaneArray | 4 | 2 | 16 | 1.969 | 507816.3 | 8.125 | 0.01075 | 0.01962 | 254.22 | 4 |
| LaneArray | 4 | 2 | 64 | 1.812 | 551856.9 | 35.319 | 0.00845 | 0.01904 | 223.00 | 4 |
| LaneArray | 4 | 2 | 256 | 1.958 | 510771.1 | 130.757 | 0.01184 | 0.01898 | 230.79 | 4 |
| LaneArray | 4 | 2 | 1024 | 6.306 | 158577.1 | 162.383 | 0.00944 | 0.01904 | 220.12 | 4 |
| LaneArray | 4 | 2 | 4096 | 23.634 | 42312.0 | 173.310 | 0.00886 | 0.02186 | 264.75 | 8 |
| LaneArray | 4 | 2 | 16384 | 93.966 | 10642.2 | 174.362 | 0.00963 | 0.02218 | 341.51 | 20 |
| LaneArray | 16 | 1 | 1 | 26.574 | 37630.7 | 0.038 | 0.01094 | 0.01981 | 351.69 | 4 |
| LaneArray | 16 | 1 | 16 | 37.269 | 26831.6 | 0.429 | 0.01178 | 0.02989 | 302.74 | 4 |
| LaneArray | 16 | 1 | 64 | 73.306 | 13641.4 | 0.873 | 0.00877 | 0.02115 | 304.17 | 4 |
| LaneArray | 16 | 1 | 256 | 75.584 | 13230.3 | 3.387 | 0.01050 | 0.02522 | 289.99 | 4 |
| LaneArray | 16 | 1 | 1024 | 71.768 | 13933.8 | 14.268 | 0.01056 | 0.01958 | 341.12 | 8 |
| LaneArray | 16 | 1 | 4096 | 152.961 | 6537.6 | 26.778 | 0.01082 | 0.03194 | 371.77 | 18 |
| LaneArray | 16 | 1 | 16384 | 282.311 | 3542.2 | 58.035 | 0.00934 | 0.03139 | 494.84 | 58 |
| LaneArray | 16 | 2 | 1 | 2.850 | 350930.4 | 0.351 | 0.01014 | 0.02566 | 321.18 | 4 |
| LaneArray | 16 | 2 | 16 | 3.242 | 308453.1 | 4.935 | 0.00877 | 0.02502 | 294.42 | 4 |
| LaneArray | 16 | 2 | 64 | 4.993 | 200279.4 | 12.818 | 0.00925 | 0.02192 | 251.80 | 4 |
| LaneArray | 16 | 2 | 256 | 7.851 | 127369.1 | 32.606 | 0.01107 | 0.02134 | 243.24 | 4 |
| LaneArray | 16 | 2 | 1024 | 27.416 | 36475.0 | 37.350 | 0.00835 | 0.03082 | 256.88 | 8 |
| LaneArray | 16 | 2 | 4096 | 98.172 | 10186.2 | 41.723 | 0.00989 | 0.03059 | 325.90 | 18 |
| LaneArray | 16 | 2 | 16384 | 931.337 | 1073.7 | 17.592 | 0.00822 | 0.03261 | 1191.46 | 58 |
| LaneArray | 64 | 1 | 1 | 121.753 | 8213.4 | 0.008 | 0.01165 | 0.02438 | 429.30 | 42 |
| LaneArray | 64 | 1 | 16 | 248.613 | 4022.3 | 0.064 | 0.00842 | 0.03661 | 462.69 | 42 |
| LaneArray | 64 | 1 | 64 | 261.419 | 3825.3 | 0.245 | 0.00950 | 0.03686 | 489.00 | 42 |
| LaneArray | 64 | 1 | 256 | 284.420 | 3515.9 | 0.900 | 0.01075 | 0.03370 | 490.71 | 46 |
| LaneArray | 64 | 1 | 1024 | 519.602 | 1924.5 | 1.971 | 0.00848 | 0.03642 | 777.67 | 56 |
| LaneArray | 64 | 1 | 4096 | 619.605 | 1613.9 | 6.611 | 0.00848 | 0.03274 | 837.86 | 96 |
| LaneArray | 64 | 1 | 16384 | 1164.445 | 858.8 | 14.070 | 0.00854 | 0.05594 | 1397.07 | 260 |
| LaneArray | 64 | 2 | 1 | 6.719 | 148830.1 | 0.149 | 0.00861 | 0.02330 | 350.42 | 4 |
| LaneArray | 64 | 2 | 16 | 10.251 | 97549.3 | 1.561 | 0.00954 | 0.02099 | 306.84 | 4 |
| LaneArray | 64 | 2 | 64 | 14.972 | 66790.8 | 4.275 | 0.00819 | 0.02022 | 264.25 | 4 |
| LaneArray | 64 | 2 | 256 | 27.881 | 35866.8 | 9.182 | 0.00874 | 0.02074 | 239.25 | 8 |
| LaneArray | 64 | 2 | 1024 | 391.434 | 2554.7 | 2.616 | 0.00880 | 0.03379 | 611.09 | 18 |
| LaneArray | 64 | 2 | 4096 | 1660.121 | 602.4 | 2.467 | 0.00854 | 0.03792 | 1880.80 | 58 |
| LaneArray | 64 | 2 | 16384 | 7375.405 | 135.6 | 2.221 | 0.00877 | 0.03315 | 7604.35 | 222 |
| SyncFifo | 8 | 1 | 1 | 0.914 | 1094647.6 | 1.095 | 0.00861 | 0.01994 | 277.16 | 4 |
| SyncFifo | 8 | 1 | 16 | 0.878 | 1138599.4 | 18.218 | 0.01210 | 0.01571 | 234.08 | 4 |
| SyncFifo | 8 | 1 | 64 | 0.799 | 1251050.9 | 80.067 | 0.00989 | 0.02013 | 223.23 | 4 |
| SyncFifo | 8 | 1 | 256 | 0.821 | 1217659.0 | 311.721 | 0.00973 | 0.02611 | 277.85 | 4 |
| SyncFifo | 8 | 1 | 1024 | 0.826 | 1210349.0 | 1239.397 | 0.00877 | 0.01933 | 213.86 | 4 |
| SyncFifo | 8 | 1 | 4096 | 0.809 | 1236644.2 | 5065.295 | 0.00941 | 0.01984 | 217.89 | 4 |
| SyncFifo | 8 | 1 | 16384 | 1.614 | 619535.7 | 10150.473 | 0.00966 | 0.01840 | 209.28 | 6 |
| SyncFifo | 64 | 1 | 1 | 0.913 | 1095453.4 | 1.095 | 0.01171 | 0.01987 | 307.06 | 4 |
| SyncFifo | 64 | 1 | 16 | 0.940 | 1063866.0 | 17.022 | 0.01005 | 0.01917 | 247.01 | 4 |
| SyncFifo | 64 | 1 | 64 | 0.846 | 1182279.1 | 75.666 | 0.00851 | 0.02208 | 257.55 | 4 |
| SyncFifo | 64 | 1 | 256 | 0.915 | 1092351.8 | 279.642 | 0.00960 | 0.01952 | 241.76 | 4 |
| SyncFifo | 64 | 1 | 1024 | 0.889 | 1125072.0 | 1152.074 | 0.00950 | 0.02125 | 207.51 | 4 |
| SyncFifo | 64 | 1 | 4096 | 0.936 | 1068558.7 | 4376.817 | 0.01245 | 0.02608 | 215.96 | 6 |
| SyncFifo | 64 | 1 | 16384 | 4.773 | 209513.6 | 3432.671 | 0.00934 | 0.01920 | 221.51 | 14 |
| SimpleDualPortRam | 16 | 1 | 1 | 0.582 | 1719300.2 | 1.719 | 0.01501 | 0.02003 | 318.33 | 4 |
| SimpleDualPortRam | 16 | 1 | 16 | 0.531 | 1884460.0 | 30.151 | 0.00957 | 0.01936 | 320.22 | 4 |
| SimpleDualPortRam | 16 | 1 | 64 | 0.469 | 2130342.9 | 136.342 | 0.01056 | 0.01965 | 248.85 | 4 |
| SimpleDualPortRam | 16 | 1 | 256 | 0.464 | 2153835.6 | 551.382 | 0.00982 | 0.01910 | 215.05 | 4 |
| SimpleDualPortRam | 16 | 1 | 1024 | 0.457 | 2190215.9 | 2242.781 | 0.01024 | 0.01965 | 262.40 | 4 |
| SimpleDualPortRam | 16 | 1 | 4096 | 0.467 | 2141584.4 | 8771.930 | 0.01014 | 0.02227 | 224.07 | 4 |
| SimpleDualPortRam | 16 | 1 | 16384 | 1.053 | 949934.6 | 15563.729 | 0.00992 | 0.01968 | 224.78 | 6 |
| SimpleDualPortRam | 256 | 1 | 1 | 0.501 | 1996294.9 | 1.996 | 0.00979 | 0.02022 | 317.76 | 4 |
| SimpleDualPortRam | 256 | 1 | 16 | 0.502 | 1992984.7 | 31.888 | 0.01152 | 0.01869 | 230.59 | 4 |
| SimpleDualPortRam | 256 | 1 | 64 | 0.564 | 1773150.2 | 113.482 | 0.00906 | 0.01856 | 240.72 | 4 |
| SimpleDualPortRam | 256 | 1 | 256 | 0.594 | 1683728.4 | 431.034 | 0.00874 | 0.01885 | 234.64 | 4 |
| SimpleDualPortRam | 256 | 1 | 1024 | 0.562 | 1777992.7 | 1820.665 | 0.01155 | 0.01926 | 277.21 | 6 |
| SimpleDualPortRam | 256 | 1 | 4096 | 1.083 | 923326.9 | 3781.947 | 0.00899 | 0.02118 | 258.00 | 12 |
| SimpleDualPortRam | 256 | 1 | 16384 | 6.369 | 157019.4 | 2572.606 | 0.00835 | 0.01891 | 262.99 | 36 |
| SimpleDualPortRam | 4096 | 1 | 1 | 0.538 | 1857575.9 | 1.858 | 0.00832 | 0.01987 | 339.62 | 4 |
| SimpleDualPortRam | 4096 | 1 | 16 | 0.672 | 1489017.0 | 23.824 | 0.00963 | 0.02022 | 266.80 | 4 |
| SimpleDualPortRam | 4096 | 1 | 64 | 0.633 | 1579798.8 | 101.107 | 0.00880 | 0.01933 | 261.45 | 6 |
| SimpleDualPortRam | 4096 | 1 | 256 | 0.715 | 1398961.4 | 358.134 | 0.01021 | 0.01974 | 229.44 | 12 |
| SimpleDualPortRam | 4096 | 1 | 1024 | 0.874 | 1144353.3 | 1171.818 | 0.00957 | 0.01869 | 213.07 | 36 |
| SimpleDualPortRam | 4096 | 1 | 4096 | 1.698 | 589000.3 | 2412.545 | 0.00963 | 0.01878 | 255.19 | 132 |
| SimpleDualPortRam | 4096 | 1 | 16384 | 7.180 | 139273.3 | 2281.853 | 0.00832 | 0.02006 | 211.55 | 516 |
| RoundRobinArbiter4 | 1 | 1 | 1 | 0.752 | 1330353.3 | 1.330 | 0.01088 | 0.02381 | 314.98 | 4 |
| RoundRobinArbiter4 | 1 | 1 | 16 | 0.713 | 1403359.1 | 22.454 | 0.00874 | 0.01910 | 287.95 | 4 |
| RoundRobinArbiter4 | 1 | 1 | 64 | 0.776 | 1288287.9 | 82.450 | 0.01229 | 0.02525 | 275.56 | 4 |
| RoundRobinArbiter4 | 1 | 1 | 256 | 0.707 | 1415115.7 | 362.270 | 0.00886 | 0.02192 | 261.27 | 4 |
| RoundRobinArbiter4 | 1 | 1 | 1024 | 0.709 | 1411217.5 | 1445.087 | 0.00890 | 0.01910 | 235.62 | 4 |
| RoundRobinArbiter4 | 1 | 1 | 4096 | 0.871 | 1148601.5 | 4704.672 | 0.01786 | 0.05402 | 309.83 | 4 |
| RoundRobinArbiter4 | 1 | 1 | 16384 | 1.010 | 990083.3 | 16221.525 | 0.00957 | 0.01920 | 291.55 | 4 |

## 交叉点与观测代价

| N | Mode 2 胜出的实测 B（按 kernel 时间） |
|---:|---|
| 1 | 256 |
| 4 | 1, 16, 64, 256, 1024 |
| 16 | 1, 16, 64, 256, 1024, 4096 |
| 64 | 1, 16, 64, 256, 1024 |

N 增大时，Mode 2 的编译器处理单份模板，槽位表随 N 增长。N=64 的共享模板编译加速 7.42×。较大 B 使 Mode 1 的线程可以并行执行多个 DUT。共享映射还需要等待块内同步，且不同 lane 的访存按槽位分散。表中交叉点反映两种映射的综合成本。未对每个成本分别做硬件计数器归因。

在 B=16,384，RAM 深度 16、256、4096 的增量显存分别为 6, 36, 516 MiB，随机读写吞吐依次为 15563.7, 2572.6, 2281.9 Mcycles/s。从深度 16 至 256 出现明显下降，继续增加 16 倍深度主要增加驻留存储。此处测量的是整轨迹吞吐。

观测实验采用 N=16、B=16,384、K=128。每拍输出含 13 个 64 位字/batch，每次 D2H 拷贝 1,703,936 字节到页锁定主存。逐拍模式保留设备寄存器和 RAM，每拍发起 kernel、打包全部顶层输出并同步拷贝。两种观测的最终校验和相同。

| 观测 | kernel ms | D2H 字节 | D2H ms | 有效 D2H GB/s | hot ms |
|---|---:|---:|---:|---:|---:|
| 末拍摘要 | 36.640 | 16 | 0.01869 | — | 269.19 |
| 每拍顶层输出 | 68.386 | 218103824 | 16.6091 | 13.13 | 313.13 |

## 故障捕获与 VCD

`FaultProbe.stage.guard.assert[0]` 在上一个时钟沿采样的 48 位字等于十进制 89223500896668 时失败。B=4,096、K=1,000 的扫描返回 `(batch=173, cycle=38, assert_id=0)`。首个匹配字出现在该 batch 的第 37 拍。对这一 batch 自动重放，T2 命中相同断言路径及拍号，并生成 15,758 字节 VCD，包含失败边沿。

| 冷启动至收到失败摘要 ms | 其中 nvcc ms | 热二进制墙钟 ms | 扫描 kernel ms | CPU 重放至临时 VCD 同步 ms | 冷启动至临时 VCD 同步 ms |
|---:|---:|---:|---:|---:|---:|
| 3272.259 | 2875.811 | 396.345 | 0.7852 | 0.4271 | 3272.691 |

冷启动计时从已 elaborated 的故障设计降级前开始。GPU 在设备上选出最早逻辑失败，主机在 K 拍扫描完成后收到摘要。因此 time-to-first-failure 使用“收到摘要”的墙钟。CPU 重放计时包含全部波形采样、缓冲写出及对临时 VCD 调用 `sync_all()`。本机临时目录 `/tmp` 挂载为 tmpfs。基准在计时结束后将该文件复制到命令行指定的目标路径。

## 原始 JSON 摘要

以下为 101 条 JSONL 测量的紧凑数组，保持数值与 runner 输出精度。时间单位为毫秒，存储单位为 MiB。

```json
[
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.02304,"device_used_mib":180,"digest":3181026177518,"elapsed_ms":1.592992,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010432,"hot_wall_ms":323.095524,"peak_vram_mib":4,"reduction_ms":0.010304,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018624,"device_used_mib":180,"digest":918966420967755,"elapsed_ms":1.330688,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010784,"hot_wall_ms":228.60639799999998,"peak_vram_mib":4,"reduction_ms":0.009472,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019008,"device_used_mib":180,"digest":12699886048853102,"elapsed_ms":1.316576,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009856,"hot_wall_ms":230.208728,"peak_vram_mib":4,"reduction_ms":0.008832,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01888,"device_used_mib":180,"digest":226853284351647231,"elapsed_ms":1.366272,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011104,"hot_wall_ms":220.58783200000002,"peak_vram_mib":4,"reduction_ms":0.01024,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01872,"device_used_mib":180,"digest":3444402419348976602,"elapsed_ms":1.350624,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008672,"hot_wall_ms":212.20509700000002,"peak_vram_mib":4,"reduction_ms":0.015424,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018848,"device_used_mib":180,"digest":17449995358826587741,"elapsed_ms":1.331616,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00864,"hot_wall_ms":245.989911,"peak_vram_mib":4,"reduction_ms":0.013824,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1985.4491620000001,"gen_ms":0.024085000000000002,"lower_ms":0.058369000000000004,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020352,"device_used_mib":186,"digest":16708041461779512939,"elapsed_ms":2.9696,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009728,"hot_wall_ms":198.775103,"peak_vram_mib":10,"reduction_ms":0.013088,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020032,"device_used_mib":180,"digest":3181026177518,"elapsed_ms":2.154976,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010784,"hot_wall_ms":314.986546,"peak_vram_mib":4,"reduction_ms":0.008416,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020096,"device_used_mib":180,"digest":918966420967755,"elapsed_ms":1.461952,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009472,"hot_wall_ms":241.978922,"peak_vram_mib":4,"reduction_ms":0.009248,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01872,"device_used_mib":180,"digest":12699886048853102,"elapsed_ms":1.32064,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00864,"hot_wall_ms":228.440225,"peak_vram_mib":4,"reduction_ms":0.007904,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01936,"device_used_mib":180,"digest":226853284351647231,"elapsed_ms":1.21856,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00928,"hot_wall_ms":223.556761,"peak_vram_mib":4,"reduction_ms":0.008192,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01984,"device_used_mib":180,"digest":3444402419348976602,"elapsed_ms":3.50256,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010528,"hot_wall_ms":223.58772900000002,"peak_vram_mib":4,"reduction_ms":0.009472,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019744,"device_used_mib":180,"digest":17449995358826587741,"elapsed_ms":8.560576,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009408,"hot_wall_ms":211.37134899999998,"peak_vram_mib":4,"reduction_ms":0.013984,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":1997.879488,"gen_ms":0.06271700000000001,"lower_ms":0.058369000000000004,"mode":2,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018528,"device_used_mib":186,"digest":16708041461779512939,"elapsed_ms":31.47776,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009728,"hot_wall_ms":243.98187,"peak_vram_mib":10,"reduction_ms":0.024928,"samples":[]},"size":1},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020224,"device_used_mib":180,"digest":6118421769662681940,"elapsed_ms":5.906304,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011424,"hot_wall_ms":325.619115,"peak_vram_mib":4,"reduction_ms":0.010144,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019104,"device_used_mib":180,"digest":11722751964833826497,"elapsed_ms":5.218304,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009248,"hot_wall_ms":252.986633,"peak_vram_mib":4,"reduction_ms":0.011648,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01888,"device_used_mib":180,"digest":5451253908212628601,"elapsed_ms":5.588992,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.0096,"hot_wall_ms":218.853341,"peak_vram_mib":4,"reduction_ms":0.013056,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.027936,"device_used_mib":180,"digest":10045927679415617202,"elapsed_ms":16.880159,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009088,"hot_wall_ms":240.05753399999998,"peak_vram_mib":4,"reduction_ms":0.032736,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020736,"device_used_mib":180,"digest":4158883152129325401,"elapsed_ms":15.723104,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008864,"hot_wall_ms":248.951922,"peak_vram_mib":4,"reduction_ms":0.019168,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.024256,"device_used_mib":184,"digest":9491947095219032881,"elapsed_ms":15.468544,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009312,"hot_wall_ms":259.98956200000003,"peak_vram_mib":8,"reduction_ms":0.020736,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2136.2720839999997,"gen_ms":0.07283600000000001,"lower_ms":0.157534,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.023936,"device_used_mib":196,"digest":10851570240827819773,"elapsed_ms":62.795776,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008672,"hot_wall_ms":278.240814,"peak_vram_mib":20,"reduction_ms":0.026624,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020064,"device_used_mib":180,"digest":6118421769662681940,"elapsed_ms":1.851968,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01024,"hot_wall_ms":282.94975900000003,"peak_vram_mib":4,"reduction_ms":0.009248,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019616,"device_used_mib":180,"digest":11722751964833826497,"elapsed_ms":1.969216,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010752,"hot_wall_ms":254.224911,"peak_vram_mib":4,"reduction_ms":0.00848,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01904,"device_used_mib":180,"digest":5451253908212628601,"elapsed_ms":1.812064,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008448,"hot_wall_ms":223.003243,"peak_vram_mib":4,"reduction_ms":0.014336,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018976,"device_used_mib":180,"digest":10045927679415617202,"elapsed_ms":1.957824,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01184,"hot_wall_ms":230.791618,"peak_vram_mib":4,"reduction_ms":0.012288,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01904,"device_used_mib":180,"digest":4158883152129325401,"elapsed_ms":6.30608,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00944,"hot_wall_ms":220.119291,"peak_vram_mib":4,"reduction_ms":0.016992,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.021856,"device_used_mib":184,"digest":9491947095219032881,"elapsed_ms":23.633984,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008864,"hot_wall_ms":264.752087,"peak_vram_mib":8,"reduction_ms":0.016384,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":2049.706916,"gen_ms":0.159077,"lower_ms":0.157534,"mode":2,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.022176,"device_used_mib":196,"digest":10851570240827819773,"elapsed_ms":93.96563,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009632,"hot_wall_ms":341.50856400000004,"peak_vram_mib":20,"reduction_ms":0.021248,"samples":[]},"size":4},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019808,"device_used_mib":180,"digest":1647948885360150067,"elapsed_ms":26.574017,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010944,"hot_wall_ms":351.686936,"peak_vram_mib":4,"reduction_ms":0.012096,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.029888,"device_used_mib":180,"digest":13175268089624299075,"elapsed_ms":37.26944,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011776,"hot_wall_ms":302.73582100000004,"peak_vram_mib":4,"reduction_ms":0.024768,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.021152,"device_used_mib":180,"digest":2833859463068238783,"elapsed_ms":73.306305,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008768,"hot_wall_ms":304.171119,"peak_vram_mib":4,"reduction_ms":0.014208,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.025216,"device_used_mib":180,"digest":11337519405698505507,"elapsed_ms":75.583809,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010496,"hot_wall_ms":289.994912,"peak_vram_mib":4,"reduction_ms":0.016384,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019584,"device_used_mib":184,"digest":8110582510776624134,"elapsed_ms":71.768066,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01056,"hot_wall_ms":341.12331,"peak_vram_mib":8,"reduction_ms":0.024352,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.031936,"device_used_mib":194,"digest":13069920926783506075,"elapsed_ms":152.961243,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010816,"hot_wall_ms":371.76898200000005,"peak_vram_mib":18,"reduction_ms":0.053184,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":3479.929078,"gen_ms":0.375431,"lower_ms":0.6259180000000001,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.031392,"device_used_mib":234,"digest":4926696464909783941,"elapsed_ms":282.310669,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009344,"hot_wall_ms":494.84164499999997,"peak_vram_mib":58,"reduction_ms":0.062048,"samples":[]},"size":16},
{"case":"Observation","mode":"summary","run":{"batch":16384,"cycles":128,"d2h_bytes":16,"d2h_ms":0.018688,"device_used_mib":234,"digest":3298423182934639940,"elapsed_ms":36.640255,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011776,"hot_wall_ms":269.188301,"peak_vram_mib":58,"reduction_ms":0.02528,"samples":[]},"size":16},
{"case":"Observation","mode":"each","run":{"batch":16384,"cycles":128,"d2h_bytes":218103824,"d2h_ms":16.60912,"device_used_mib":234,"digest":3298423182934639940,"elapsed_ms":68.386272,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010816,"hot_wall_ms":313.12872400000003,"peak_vram_mib":58,"reduction_ms":0.026432,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.025664,"device_used_mib":180,"digest":1647948885360150067,"elapsed_ms":2.849568,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010144,"hot_wall_ms":321.17627,"peak_vram_mib":4,"reduction_ms":0.009088,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.025024,"device_used_mib":180,"digest":13175268089624299075,"elapsed_ms":3.241984,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008768,"hot_wall_ms":294.41666100000003,"peak_vram_mib":4,"reduction_ms":0.010016,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.02192,"device_used_mib":180,"digest":2833859463068238783,"elapsed_ms":4.993024,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009248,"hot_wall_ms":251.79589,"peak_vram_mib":4,"reduction_ms":0.017984,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.021344,"device_used_mib":180,"digest":11337519405698505507,"elapsed_ms":7.8512,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011072,"hot_wall_ms":243.237438,"peak_vram_mib":4,"reduction_ms":0.01184,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.030816,"device_used_mib":184,"digest":8110582510776624134,"elapsed_ms":27.416033,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008352,"hot_wall_ms":256.87639099999996,"peak_vram_mib":8,"reduction_ms":0.059328,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.030592,"device_used_mib":194,"digest":13069920926783506075,"elapsed_ms":98.171806,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009888,"hot_wall_ms":325.901428,"peak_vram_mib":18,"reduction_ms":0.057216,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":2075.527931,"gen_ms":0.492039,"lower_ms":0.6259180000000001,"mode":2,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.032608,"device_used_mib":234,"digest":4926696464909783941,"elapsed_ms":931.337036,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008224,"hot_wall_ms":1191.4639339999999,"peak_vram_mib":58,"reduction_ms":0.064928,"samples":[]},"size":16},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.024384,"device_used_mib":218,"digest":4065402579303973255,"elapsed_ms":121.75267,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011648,"hot_wall_ms":429.302615,"peak_vram_mib":42,"reduction_ms":0.013856,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.036608,"device_used_mib":218,"digest":10711133131116911501,"elapsed_ms":248.612961,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008416,"hot_wall_ms":462.68942000000004,"peak_vram_mib":42,"reduction_ms":0.045504,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.036864,"device_used_mib":218,"digest":1700849607353329071,"elapsed_ms":261.418762,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009504,"hot_wall_ms":488.996694,"peak_vram_mib":42,"reduction_ms":0.038944,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.033696,"device_used_mib":222,"digest":3677493477953057488,"elapsed_ms":284.420288,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010752,"hot_wall_ms":490.713937,"peak_vram_mib":46,"reduction_ms":0.057344,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.036416,"device_used_mib":232,"digest":10549789195859956587,"elapsed_ms":519.602112,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00848,"hot_wall_ms":777.666901,"peak_vram_mib":56,"reduction_ms":0.065504,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.032736,"device_used_mib":272,"digest":10666705786897089488,"elapsed_ms":619.605103,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00848,"hot_wall_ms":837.858746,"peak_vram_mib":96,"reduction_ms":0.068128,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":17155.216876000002,"gen_ms":0.87349,"lower_ms":1.5333919999999999,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.055936,"device_used_mib":436,"digest":7240126056558415849,"elapsed_ms":1164.445068,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008544,"hot_wall_ms":1397.071248,"peak_vram_mib":260,"reduction_ms":0.097024,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.023296,"device_used_mib":180,"digest":4065402579303973255,"elapsed_ms":6.719072,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008608,"hot_wall_ms":350.421272,"peak_vram_mib":4,"reduction_ms":0.019264,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020992,"device_used_mib":180,"digest":10711133131116911501,"elapsed_ms":10.251232,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009536,"hot_wall_ms":306.842085,"peak_vram_mib":4,"reduction_ms":0.016672,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020224,"device_used_mib":180,"digest":1700849607353329071,"elapsed_ms":14.972128,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008192,"hot_wall_ms":264.253539,"peak_vram_mib":4,"reduction_ms":0.018432,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020736,"device_used_mib":184,"digest":3677493477953057488,"elapsed_ms":27.88096,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008736,"hot_wall_ms":239.254326,"peak_vram_mib":8,"reduction_ms":0.023936,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.033792,"device_used_mib":194,"digest":10549789195859956587,"elapsed_ms":391.433838,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.0088,"hot_wall_ms":611.088335,"peak_vram_mib":18,"reduction_ms":0.06048,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.03792,"device_used_mib":234,"digest":10666705786897089488,"elapsed_ms":1660.120605,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008544,"hot_wall_ms":1880.800137,"peak_vram_mib":58,"reduction_ms":0.058816,"samples":[]},"size":64},
{"case":"LaneArray","cold_ms":2310.822349,"gen_ms":2.866389,"lower_ms":1.5333919999999999,"mode":2,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.033152,"device_used_mib":398,"digest":7240126056558415849,"elapsed_ms":7375.404785,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008768,"hot_wall_ms":7604.346291,"peak_vram_mib":222,"reduction_ms":0.080192,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019936,"device_used_mib":180,"digest":248,"elapsed_ms":0.913536,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008608,"hot_wall_ms":277.157209,"peak_vram_mib":4,"reduction_ms":0.009664,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.015712,"device_used_mib":180,"digest":38678,"elapsed_ms":0.878272,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.012096,"hot_wall_ms":234.076425,"peak_vram_mib":4,"reduction_ms":0.008192,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020128,"device_used_mib":180,"digest":560149,"elapsed_ms":0.799328,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009888,"hot_wall_ms":223.228642,"peak_vram_mib":4,"reduction_ms":0.008192,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.026112,"device_used_mib":180,"digest":10283730,"elapsed_ms":0.821248,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009728,"hot_wall_ms":277.846304,"peak_vram_mib":4,"reduction_ms":0.010016,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019328,"device_used_mib":180,"digest":153920594,"elapsed_ms":0.826208,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008768,"hot_wall_ms":213.856833,"peak_vram_mib":4,"reduction_ms":0.009408,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01984,"device_used_mib":180,"digest":2534175380,"elapsed_ms":0.80864,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009408,"hot_wall_ms":217.89134099999998,"peak_vram_mib":4,"reduction_ms":0.012288,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2070.140732,"gen_ms":0.026019,"lower_ms":0.042309,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.0184,"device_used_mib":182,"digest":39546963237,"elapsed_ms":1.614112,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009664,"hot_wall_ms":209.27706600000002,"peak_vram_mib":6,"reduction_ms":0.014688,"samples":[]},"size":8},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019872,"device_used_mib":180,"digest":308,"elapsed_ms":0.912864,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011712,"hot_wall_ms":307.058606,"peak_vram_mib":4,"reduction_ms":0.008192,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019168,"device_used_mib":180,"digest":60477,"elapsed_ms":0.939968,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010048,"hot_wall_ms":247.013785,"peak_vram_mib":4,"reduction_ms":0.009376,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.02208,"device_used_mib":180,"digest":965712,"elapsed_ms":0.845824,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008512,"hot_wall_ms":257.545017,"peak_vram_mib":4,"reduction_ms":0.008096,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01952,"device_used_mib":180,"digest":16188671,"elapsed_ms":0.915456,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.0096,"hot_wall_ms":241.76483,"peak_vram_mib":4,"reduction_ms":0.009152,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.021248,"device_used_mib":180,"digest":261181789,"elapsed_ms":0.888832,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009504,"hot_wall_ms":207.509546,"peak_vram_mib":4,"reduction_ms":0.010048,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.02608,"device_used_mib":182,"digest":4166044431,"elapsed_ms":0.93584,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.012448,"hot_wall_ms":215.955877,"peak_vram_mib":6,"reduction_ms":0.017152,"samples":[]},"size":64},
{"case":"SyncFifo","cold_ms":2013.8006229999999,"gen_ms":0.035215,"lower_ms":0.051386,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.0192,"device_used_mib":190,"digest":66496895439,"elapsed_ms":4.77296,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009344,"hot_wall_ms":221.505163,"peak_vram_mib":14,"reduction_ms":0.013792,"samples":[]},"size":64},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020032,"device_used_mib":180,"digest":55,"elapsed_ms":0.581632,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.015008,"hot_wall_ms":318.328874,"peak_vram_mib":4,"reduction_ms":0.016576,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01936,"device_used_mib":180,"digest":29929,"elapsed_ms":0.530656,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009568,"hot_wall_ms":320.221176,"peak_vram_mib":4,"reduction_ms":0.012288,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019648,"device_used_mib":180,"digest":543212,"elapsed_ms":0.469408,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01056,"hot_wall_ms":248.846562,"peak_vram_mib":4,"reduction_ms":0.010272,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019104,"device_used_mib":180,"digest":8511005,"elapsed_ms":0.464288,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009824,"hot_wall_ms":215.045747,"peak_vram_mib":4,"reduction_ms":0.01008,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019648,"device_used_mib":180,"digest":129888723,"elapsed_ms":0.456576,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01024,"hot_wall_ms":262.396919,"peak_vram_mib":4,"reduction_ms":0.01008,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.022272,"device_used_mib":180,"digest":2114267133,"elapsed_ms":0.466944,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010144,"hot_wall_ms":224.072652,"peak_vram_mib":4,"reduction_ms":0.012096,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":1954.190254,"gen_ms":0.01584,"lower_ms":0.026690000000000002,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01968,"device_used_mib":182,"digest":34127700521,"elapsed_ms":1.052704,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00992,"hot_wall_ms":224.78455,"peak_vram_mib":6,"reduction_ms":0.012992,"samples":[]},"size":16},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020224,"device_used_mib":180,"digest":139,"elapsed_ms":0.500928,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009792,"hot_wall_ms":317.758747,"peak_vram_mib":4,"reduction_ms":0.01024,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018688,"device_used_mib":180,"digest":16096,"elapsed_ms":0.50176,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01152,"hot_wall_ms":230.59141499999998,"peak_vram_mib":4,"reduction_ms":0.010144,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.01856,"device_used_mib":180,"digest":525419,"elapsed_ms":0.563968,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009056,"hot_wall_ms":240.720918,"peak_vram_mib":4,"reduction_ms":0.010144,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018848,"device_used_mib":180,"digest":7348687,"elapsed_ms":0.59392,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008736,"hot_wall_ms":234.641882,"peak_vram_mib":4,"reduction_ms":0.009792,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019264,"device_used_mib":182,"digest":114818018,"elapsed_ms":0.562432,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.011552,"hot_wall_ms":277.20921799999996,"peak_vram_mib":6,"reduction_ms":0.008832,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.021184,"device_used_mib":188,"digest":1850471980,"elapsed_ms":1.08304,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008992,"hot_wall_ms":258.002526,"peak_vram_mib":12,"reduction_ms":0.01024,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":2135.168407,"gen_ms":0.017221999999999998,"lower_ms":0.029926,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018912,"device_used_mib":212,"digest":29347256071,"elapsed_ms":6.36864,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008352,"hot_wall_ms":262.992958,"peak_vram_mib":36,"reduction_ms":0.012288,"samples":[]},"size":256},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019872,"device_used_mib":180,"digest":0,"elapsed_ms":0.538336,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00832,"hot_wall_ms":339.623475,"peak_vram_mib":4,"reduction_ms":0.01024,"samples":[]},"size":4096},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020224,"device_used_mib":180,"digest":4371,"elapsed_ms":0.671584,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009632,"hot_wall_ms":266.80462900000003,"peak_vram_mib":4,"reduction_ms":0.01056,"samples":[]},"size":4096},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019328,"device_used_mib":182,"digest":48091,"elapsed_ms":0.632992,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.0088,"hot_wall_ms":261.449345,"peak_vram_mib":6,"reduction_ms":0.008288,"samples":[]},"size":4096},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019744,"device_used_mib":188,"digest":658031,"elapsed_ms":0.714816,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.010208,"hot_wall_ms":229.440825,"peak_vram_mib":12,"reduction_ms":0.007904,"samples":[]},"size":4096},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018688,"device_used_mib":212,"digest":15828991,"elapsed_ms":0.873856,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009568,"hot_wall_ms":213.067231,"peak_vram_mib":36,"reduction_ms":0.007552,"samples":[]},"size":4096},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.018784,"device_used_mib":308,"digest":259178834,"elapsed_ms":1.697792,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009632,"hot_wall_ms":255.194834,"peak_vram_mib":132,"reduction_ms":0.009696,"samples":[]},"size":4096},
{"case":"SimpleDualPortRam","cold_ms":1972.7317950000001,"gen_ms":0.017503,"lower_ms":0.029114,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.020064,"device_used_mib":692,"digest":3917391825,"elapsed_ms":7.180128,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.00832,"hot_wall_ms":211.55148200000002,"peak_vram_mib":516,"reduction_ms":0.017504,"samples":[]},"size":4096},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":1,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.023808,"device_used_mib":180,"digest":4,"elapsed_ms":0.75168,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.01088,"hot_wall_ms":314.980056,"peak_vram_mib":4,"reduction_ms":0.010528,"samples":[]},"size":1},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":16,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019104,"device_used_mib":180,"digest":3294,"elapsed_ms":0.712576,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008736,"hot_wall_ms":287.954974,"peak_vram_mib":4,"reduction_ms":0.009856,"samples":[]},"size":1},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":64,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.025248,"device_used_mib":180,"digest":52424,"elapsed_ms":0.776224,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.012288,"hot_wall_ms":275.564262,"peak_vram_mib":4,"reduction_ms":0.00912,"samples":[]},"size":1},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":256,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.02192,"device_used_mib":180,"digest":890373,"elapsed_ms":0.706656,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008864,"hot_wall_ms":261.26579,"peak_vram_mib":4,"reduction_ms":0.008352,"samples":[]},"size":1},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":1024,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.019104,"device_used_mib":180,"digest":13930465,"elapsed_ms":0.708608,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.008896,"hot_wall_ms":235.61797,"peak_vram_mib":4,"reduction_ms":0.0112,"samples":[]},"size":1},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.054016,"device_used_mib":180,"digest":226224142,"elapsed_ms":0.870624,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.017856,"hot_wall_ms":309.829814,"peak_vram_mib":4,"reduction_ms":0.023456,"samples":[]},"size":1},
{"case":"RoundRobinArbiter4","cold_ms":2030.263975,"gen_ms":0.036549,"lower_ms":0.038592,"mode":1,"run":{"batch":16384,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.0192,"device_used_mib":180,"digest":3600405038,"elapsed_ms":1.010016,"fail_assert":null,"fail_batch":null,"fail_cycle":null,"h2d_ms":0.009568,"hot_wall_ms":291.552777,"peak_vram_mib":4,"reduction_ms":0.01312,"samples":[]},"size":1},
{"assert_path":"FaultProbe.stage.guard.assert[0]","case":"FaultProbe","cold_to_detection_ms":3272.259442,"compile_ms":2875.8111369999997,"end_to_end_ms":3272.691388,"gen_ms":0.022993000000000003,"replay_vcd_ms":0.42710699999999996,"run":{"batch":4096,"cycles":1000,"d2h_bytes":16,"d2h_ms":0.078688,"device_used_mib":180,"digest":15163926432975956465,"elapsed_ms":0.785248,"fail_assert":0,"fail_batch":173,"fail_cycle":38,"h2d_ms":0.01072,"hot_wall_ms":396.345204,"peak_vram_mib":4,"reduction_ms":0.02224,"samples":[]},"vcd_bytes":15758}
]
```
