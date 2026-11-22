//! Reproducible two-dimensional CPU batch benchmark.
//! Run with `cargo run -p syl_sim --release --example bench_cpu_2d`.
use std::collections::BTreeMap;
use std::hint::black_box;
use std::time::Instant;
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::{BatchCpuSimulator, ParamValue, SimDesign, Simulator, Value};
use syl_span::SourceId;
use syl_syntax::SourceParser;

fn compile(files: &[(&str, &str)]) -> ElaborationOutput {
    let ast: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(i, (_, s))| {
            SourceParser::new_in(s, SourceId::new(i))
                .parse_file()
                .unwrap()
        })
        .collect();
    let files = files
        .iter()
        .zip(&ast)
        .map(|((path, _), ast)| {
            SemanticSourceFile::new(path.split('.').map(str::to_owned).collect(), ast)
        })
        .collect();
    let hir = SemanticSession::new_sources(files).resolve_hir().unwrap();
    let tir = hir.check_tir().unwrap();
    HardwareCompiler::new().output_for_tir(&tir)
}
fn splitmix(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e3779b97f4a7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
    z ^ (z >> 31)
}
fn value(width: u32, n: u64) -> Value {
    Value::from_u64(width.min(64), n).resized(width)
}
fn drive_batch(batch: &mut BatchCpuSimulator<'_>, cycle: u64) {
    for lane in 0..batch.batch_size() {
        let mut seed = cycle ^ (lane as u64).wrapping_mul(0x517cc1b727220a95);
        for port in batch.design().top_ports() {
            if port.direction == syl_sim::HwDirection::Out || port.name == "clk" {
                continue;
            }
            let n = splitmix(&mut seed);
            let _ = batch.drive(
                lane,
                port.signal,
                value(batch.design().signal(port.signal).width, n),
            );
        }
    }
}
fn drive_scalar(sim: &mut Simulator<'_>, cycle: u64, lane: usize) {
    let mut seed = cycle ^ (lane as u64).wrapping_mul(0x517cc1b727220a95);
    for port in sim.design().top_ports() {
        if port.direction == syl_sim::HwDirection::Out || port.name == "clk" {
            continue;
        }
        let n = splitmix(&mut seed);
        let _ = sim.drive(
            port.signal,
            value(sim.design().signal(port.signal).width, n),
        );
    }
}
fn run(design: &SimDesign, batch_size: usize, cycles: u64) -> (f64, f64) {
    let mut batch = BatchCpuSimulator::new(design, batch_size);
    let start = Instant::now();
    for cycle in 0..cycles {
        drive_batch(&mut batch, cycle);
        let _ = batch.eval_comb();
        if !design.clocks.is_empty() {
            let _ = batch.tick("clk");
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    black_box(
        batch.read_batch(
            design
                .top_ports()
                .first()
                .map_or(syl_sim::SignalId(0), |p| p.signal),
        ),
    );
    (elapsed, batch_size as f64 * cycles as f64 / elapsed)
}
fn run_scalar(design: &SimDesign, batch_size: usize, cycles: u64) -> (f64, f64) {
    let mut sims: Vec<_> = (0..batch_size)
        .map(|_| Simulator::with_mode(design, syl_sim::StateMode::TwoState))
        .collect();
    let start = Instant::now();
    for cycle in 0..cycles {
        for (lane, sim) in sims.iter_mut().enumerate() {
            drive_scalar(sim, cycle, lane);
            let _ = sim.eval_comb();
            if !design.clocks.is_empty() {
                let _ = sim.tick("clk");
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    black_box(sims.len());
    (elapsed, batch_size as f64 * cycles as f64 / elapsed)
}
fn main() {
    let stream = include_str!("../../../examples/std/stream.syl");
    let arbiter = compile(&[(
        "std.arbiter",
        include_str!("../../../examples/std/arbiter.syl"),
    )]);
    let fifo = compile(&[
        ("std.stream", stream),
        ("std.fifo", include_str!("../../../examples/std/fifo.syl")),
    ]);
    let ram = compile(&[("std.mem", include_str!("../../../examples/std/mem.syl"))]);
    let lanes = compile(&[
        ("std.stream", stream),
        ("lane", include_str!("../../../examples/mvp/lane_array.syl")),
    ]);
    let mut designs = vec![
        (
            "RoundRobinArbiter4",
            SimDesign::lower(&arbiter, "RoundRobinArbiter4", &BTreeMap::new()).unwrap(),
        ),
        (
            "SyncFifo-D8",
            SimDesign::lower(&fifo, "SyncFifo", &BTreeMap::new()).unwrap(),
        ),
        (
            "SimpleDualPortRam",
            SimDesign::lower(&ram, "SimpleDualPortRam", &BTreeMap::new()).unwrap(),
        ),
    ];
    for n in [1, 4, 16, 64] {
        designs.push((
            "LaneArray",
            SimDesign::lower(
                &lanes,
                "LaneArray",
                &BTreeMap::from([("N".into(), ParamValue::Nat(n))]),
            )
            .unwrap(),
        ));
    }
    println!("design,batch,cycles,scalar_s,scalar_aggregate_cycles_s,soa_s,soa_aggregate_cycles_s");
    for (name, design) in &designs {
        for batch in [1usize, 16, 64, 256, 1024, 4096] {
            let cycles = 4;
            let (scalar_s, scalar_t) = run_scalar(design, batch, cycles);
            let (soa_s, soa_t) = run(design, batch, cycles);
            println!("{name},{batch},{cycles},{scalar_s:.6},{scalar_t:.1},{soa_s:.6},{soa_t:.1}");
        }
    }
}
