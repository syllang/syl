//! Native driver for run.py. All engines consume the same counter-based SplitMix64 stream.
#[path = "bench_cpu_2d/workload.rs"]
mod workload;

use std::{
    fmt::Write,
    fs,
    path::Path,
    sync::atomic::{AtomicU8, AtomicU64, Ordering::Relaxed},
    time::Instant,
};
use syl_sim::{
    FastBatchCpuSimulator, FastProgram, HwDirection, ParamValue, RuntimeError, SimDesign,
    Simulator, StateMode,
};
use workload::{fixture, mix, ports, scalar_value, stimulus};

fn emit(dir: &Path, case: &str) {
    let (ir, top, params) = fixture(case);
    let sv = syl_emit::SystemVerilogBackend::new()
        .emit(ir.hwir().unwrap())
        .unwrap();
    fs::write(dir.join("design.sv"), sv).unwrap();
    let design = SimDesign::lower(&ir, top, &params).unwrap();
    let mut drive = String::new();
    let mut digest = String::new();
    for p in ports(&design, HwDirection::In) {
        let name = &design.top_ports()[p.key as usize].name;
        for w in 0..p.width.div_ceil(64) {
            let bits = (p.width - 64 * w).min(64);
            let expr = if p.reset {
                "(cycle == 1)".into()
            } else if p.inject {
                "(lane == 3073 && cycle == 500)".into()
            } else {
                format!(
                    "(random_word(lane, cycle, {}, {w}) & {}ULL)",
                    p.key,
                    u64::MAX >> (64 - bits)
                )
            };
            if p.width <= 64 {
                writeln!(drive, "d.{name} = {expr};").unwrap();
            } else {
                writeln!(
                    drive,
                    "{{ const uint64_t v = {expr}; d.{name}[{}] = uint32_t(v);",
                    w * 2
                )
                .unwrap();
                if bits > 32 {
                    writeln!(drive, "d.{name}[{}] = uint32_t(v >> 32);", w * 2 + 1).unwrap();
                }
                drive.push_str("}\n");
            }
        }
    }
    for p in ports(&design, HwDirection::Out) {
        let name = &design.top_ports()[p.key as usize].name;
        for w in 0..p.width.div_ceil(64) {
            let expr = if p.width <= 64 {
                format!("uint64_t(d.{name})")
            } else if p.width - w * 64 > 32 {
                format!(
                    "(uint64_t(d.{name}[{}]) | uint64_t(d.{name}[{}]) << 32)",
                    w * 2,
                    w * 2 + 1
                )
            } else {
                format!("uint64_t(d.{name}[{}])", w * 2)
            };
            writeln!(
                digest,
                "sum += mix({expr} ^ (uint64_t(lane) << 32) ^ {}ULL);",
                (p.key << 16) + u64::from(w)
            )
            .unwrap();
        }
    }
    fs::write(
        dir.join("harness.cpp"),
        include_str!("bench_cpu_2d/harness.cpp")
            .replace("@DRIVE@", &drive)
            .replace("@DIGEST@", &digest)
            .replace(
                "@CHECKPOINT@",
                if case == "Fault" {
                    "if (cycle == 499) checkpoint[lane] = char('0' + d.y);"
                } else {
                    ""
                },
            )
            .replace("@FAULT@", if case == "Fault" { "true" } else { "false" }),
    )
    .unwrap();
    println!(
        "{top} {}",
        params
            .iter()
            .map(|(k, v)| {
                let ParamValue::Nat(n) = v else { panic!() };
                format!("-G{k}={n}")
            })
            .collect::<Vec<_>>()
            .join(" ")
    );
}

fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let mode = &args[0];
    let case = &args[1];
    if mode == "emit" {
        emit(Path::new(&args[2]), case);
        return;
    }
    let batch: usize = args[2].parse().unwrap();
    let cycles: u64 = args[3].parse().unwrap();
    let threads: usize = args[4].parse().unwrap();
    let cold = Instant::now();
    let (ir, top, params) = fixture(case);
    let design = SimDesign::lower(&ir, top, &params).unwrap();
    let inputs = ports(&design, HwDirection::In);
    let outputs = if case == "Fault" {
        Vec::new()
    } else {
        ports(&design, HwDirection::Out)
    };
    let failure = AtomicU64::new(u64::MAX);
    let digest = AtomicU64::new(0);
    // Fault exposes its sole register q as y. Save every committed bit before
    // any trajectory attempts the failing edge, independent of early-stop granularity.
    let fault_register = (case == "Fault").then(|| {
        assert_eq!(design.regs.len(), 1);
        assert!(design.memories.is_empty());
        design.regs[0].signal
    });
    let checkpoint: Vec<_> = (0..if fault_register.is_some() { batch } else { 0 })
        .map(|_| AtomicU8::new(b'?'))
        .collect();
    let setup;
    let elapsed;
    if mode == "scalar" {
        let mut sims: Vec<_> = (0..batch)
            .map(|_| Simulator::with_mode(&design, StateMode::TwoState))
            .collect();
        let hot = Instant::now();
        setup = hot.duration_since(cold).as_secs_f64();
        'cycles: for cycle in 1..=cycles {
            for (lane, s) in sims.iter_mut().enumerate() {
                for &p in &inputs {
                    s.drive(p.id, scalar_value(p, lane, cycle)).unwrap();
                }
                if let Err(e) = s.tick("clk") {
                    let RuntimeError::Assertion(f) = e else {
                        panic!("{e}")
                    };
                    failure.store((f.cycle << 32) + lane as u64, Relaxed);
                    break 'cycles;
                }
                if cycle == 499
                    && let Some(q) = fault_register
                {
                    checkpoint[lane].store(b'0' + s.read(q).as_u64().unwrap() as u8, Relaxed);
                }
            }
        }
        for (lane, s) in sims.iter().enumerate() {
            for p in &outputs {
                for w in 0..p.width.div_ceil(64) {
                    let val = s
                        .read(p.id)
                        .slice(w * 64, (p.width - w * 64).min(64))
                        .as_u64()
                        .unwrap();
                    digest.fetch_add(
                        mix(val ^ ((lane as u64) << 32) ^ ((p.key << 16) + u64::from(w))),
                        Relaxed,
                    );
                }
            }
        }
        elapsed = hot.elapsed().as_secs_f64();
    } else {
        assert_eq!(mode, "soa");
        let program = FastProgram::new(&design).unwrap();
        let tile_size = 64.min(batch.div_ceil(threads));
        let mut sim = FastBatchCpuSimulator::new(&program, batch, tile_size);
        let first_start = AtomicU64::new(u64::MAX);
        let last_end = AtomicU64::new(0);
        sim.for_each_tile(threads, |first, tile| {
            let start = cold.elapsed().as_nanos() as u64;
            for cycle in 1..=cycles {
                for &p in &inputs {
                    for w in 0..p.width.div_ceil(64) {
                        for (i, dst) in tile
                            .input_word(&program, p.id, w as usize)
                            .iter_mut()
                            .enumerate()
                        {
                            *dst = stimulus(p, w, first + i, cycle);
                        }
                    }
                }
                if let Err((i, c)) = tile.tick(&program) {
                    failure.fetch_min((c << 32) + (first + i) as u64, Relaxed);
                    break;
                }
                if cycle == 499
                    && let Some(q) = fault_register
                {
                    for (offset, &bit) in tile.word(&program, q, 0).iter().enumerate() {
                        checkpoint[first + offset].store(b'0' + bit as u8, Relaxed);
                    }
                }
            }
            let mut sum = 0u64;
            for p in &outputs {
                for w in 0..p.width.div_ceil(64) {
                    for (i, &val) in tile.word(&program, p.id, w as usize).iter().enumerate() {
                        sum = sum.wrapping_add(mix(val
                            ^ (((first + i) as u64) << 32)
                            ^ ((p.key << 16) + u64::from(w))));
                    }
                }
            }
            digest.fetch_add(sum, Relaxed);
            let end = cold.elapsed().as_nanos() as u64;
            first_start.fetch_min(start, Relaxed);
            last_end.fetch_max(end, Relaxed);
        });
        let start = first_start.load(Relaxed);
        setup = start as f64 * 1e-9;
        elapsed = (last_end.load(Relaxed) - start) as f64 * 1e-9;
    }
    let snapshot: String = checkpoint
        .iter()
        .map(|bit| char::from(bit.load(Relaxed)))
        .collect();
    let sum = if fault_register.is_some() {
        snapshot.bytes().enumerate().fold(0u64, |sum, (lane, bit)| {
            sum.wrapping_add(mix(((lane as u64) << 1) | u64::from(bit - b'0')))
        })
    } else {
        digest.load(Relaxed)
    };
    println!(
        "{:.9} {:.9} {} {} {}",
        setup,
        elapsed,
        sum,
        failure.load(Relaxed),
        snapshot
    );
}
