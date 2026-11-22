use std::collections::BTreeMap;

use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::{BatchCpuSimulator, SimDesign, Simulator, Value};
use syl_span::SourceId;
use syl_syntax::SourceParser;

fn compile(files: &[(&str, &str)]) -> ElaborationOutput {
    let ast: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(i, (_, source))| {
            SourceParser::new_in(source, SourceId::new(i))
                .parse_file()
                .expect("fixture parses")
        })
        .collect();
    let sources = files
        .iter()
        .zip(&ast)
        .map(|((path, _), ast)| {
            SemanticSourceFile::new(path.split('.').map(str::to_owned).collect(), ast)
        })
        .collect();
    let hir = SemanticSession::new_sources(sources)
        .resolve_hir()
        .expect("fixture resolves");
    let tir = hir.check_tir().expect("fixture type checks");
    let output = HardwareCompiler::new().output_for_tir(&tir);
    assert!(output.hwir().is_some(), "{:?}", output.diagnostics());
    output
}

fn arbiter() -> SimDesign {
    SimDesign::lower(
        &compile(&[(
            "std.arbiter",
            include_str!("../../../examples/std/arbiter.syl"),
        )]),
        "RoundRobinArbiter4",
        &BTreeMap::new(),
    )
    .expect("arbiter lowers")
}

fn standard(top: &str, source: &str) -> SimDesign {
    SimDesign::lower(
        &compile(&[
            (
                "std.stream",
                include_str!("../../../examples/std/stream.syl"),
            ),
            ("fixture", source),
        ]),
        top,
        &BTreeMap::new(),
    )
    .expect("standard cell lowers")
}

fn lane_array() -> SimDesign {
    let output = compile(&[
        (
            "std.stream",
            include_str!("../../../examples/std/stream.syl"),
        ),
        ("lane", include_str!("../../../examples/mvp/lane_array.syl")),
    ]);
    let design = SimDesign::lower(
        &output,
        "LaneArray",
        &BTreeMap::from([("N".into(), syl_sim::ParamValue::Nat(1))]),
    )
    .unwrap();
    design
}

#[test]
fn batch_round_robin_matches_independent_reference_lanes() {
    let design = arbiter();
    let mut batch = BatchCpuSimulator::new(&design, 16);
    let mut reference: Vec<_> = (0..16)
        .map(|_| Simulator::with_mode(&design, syl_sim::StateMode::TwoState))
        .collect();

    assert_eq!(batch.batch_size(), 16);
    assert_eq!(batch.batch_stride(), 16);
    batch.broadcast_named("rst", Value::from_u64(1, 1)).unwrap();
    for sim in &mut reference {
        sim.drive_named("rst", Value::from_u64(1, 1)).unwrap();
    }
    batch.tick("clk").unwrap();
    for sim in &mut reference {
        sim.tick("clk").unwrap();
    }
    batch.broadcast_named("rst", Value::from_u64(1, 0)).unwrap();
    for sim in &mut reference {
        sim.drive_named("rst", Value::from_u64(1, 0)).unwrap();
    }

    for cycle in 0..64u64 {
        for (lane, sim) in reference.iter_mut().enumerate() {
            let request = ((cycle.wrapping_mul(17) + lane as u64 * 5) ^ (cycle >> 2)) & 0xf;
            let value = Value::from_u64(4, request);
            batch.drive_named(lane, "req", value.clone()).unwrap();
            sim.drive_named("req", value).unwrap();
        }
        batch.eval_comb().unwrap();
        for sim in &mut reference {
            sim.eval_comb().unwrap();
        }
        for (lane, sim) in reference.iter().enumerate() {
            for name in ["grant", "grant_valid", "grant_idx"] {
                assert_eq!(
                    batch.read_named(lane, name).unwrap(),
                    sim.read_named(name).unwrap(),
                    "cycle {cycle}, lane {lane}, output {name}"
                );
            }
        }
        batch.tick("clk").unwrap();
        for sim in &mut reference {
            sim.tick("clk").unwrap();
        }
    }
}

#[test]
fn batch_lane_array_matches_reference_lanes() {
    let design = lane_array();
    let mut batch = BatchCpuSimulator::new(&design, 4);
    let mut reference: Vec<_> = (0..4)
        .map(|_| Simulator::with_mode(&design, syl_sim::StateMode::TwoState))
        .collect();
    for lane in 0..4 {
        for (name, width, value) in [("rst", 1, 1), ("lane_ids", 32, lane as u64)] {
            let v = Value::from_u64(width, value);
            batch.drive_named(lane, name, v.clone()).unwrap();
            reference[lane].drive_named(name, v).unwrap();
        }
    }
    batch.tick("clk").unwrap();
    for sim in &mut reference {
        sim.tick("clk").unwrap();
    }
    for lane in 0..4 {
        let v = Value::from_u64(1, 0);
        batch.drive_named(lane, "rst", v.clone()).unwrap();
        reference[lane].drive_named("rst", v).unwrap();
    }
    for cycle in 0..32u64 {
        for lane in 0..4 {
            let payload = ((cycle * 13 + lane as u64 * 7) << 8) | (lane as u64 + 1);
            for (name, width, value) in [
                ("in_streams_payload", 164, payload),
                ("in_streams_valid", 4, ((cycle + lane as u64) & 3) as u64),
                (
                    "out_streams_ready",
                    4,
                    ((cycle >> 1) + lane as u64) as u64 & 0xf,
                ),
            ] {
                let v = Value::from_u64(width, value);
                batch.drive_named(lane, name, v.clone()).unwrap();
                reference[lane].drive_named(name, v).unwrap();
            }
        }
        batch.eval_comb().unwrap();
        for sim in &mut reference {
            sim.eval_comb().unwrap();
        }
        for (lane, sim) in reference.iter().enumerate() {
            for name in [
                "out_streams_payload",
                "out_streams_valid",
                "in_streams_ready",
            ] {
                assert_eq!(
                    batch.read_named(lane, name).unwrap(),
                    sim.read_named(name).unwrap(),
                    "cycle {cycle}, lane {lane}, {name}"
                );
            }
        }
        batch.tick("clk").unwrap();
        for sim in &mut reference {
            sim.tick("clk").unwrap();
        }
    }
}

#[test]
fn batch_fifo_and_ram_match_reference_lanes() {
    let fifo = standard("SyncFifo", include_str!("../../../examples/std/fifo.syl"));
    let ram = standard(
        "SimpleDualPortRam",
        include_str!("../../../examples/std/mem.syl"),
    );
    for (design, outputs) in [
        (&fifo, ["rdata", "full", "empty", "count"].as_slice()),
        (&ram, ["rdata"].as_slice()),
    ] {
        let mut batch = BatchCpuSimulator::new(design, 4);
        let mut reference: Vec<_> = (0..4)
            .map(|_| Simulator::with_mode(design, syl_sim::StateMode::TwoState))
            .collect();
        let has_reset = design.top_ports().iter().any(|p| p.name == "rst");
        if has_reset {
            for lane in 0..4 {
                batch
                    .drive_named(lane, "rst", Value::from_u64(1, 1))
                    .unwrap();
                reference[lane]
                    .drive_named("rst", Value::from_u64(1, 1))
                    .unwrap();
            }
            batch.tick("clk").unwrap();
            for sim in &mut reference {
                sim.tick("clk").unwrap();
            }
            for lane in 0..4 {
                batch
                    .drive_named(lane, "rst", Value::from_u64(1, 0))
                    .unwrap();
                reference[lane]
                    .drive_named("rst", Value::from_u64(1, 0))
                    .unwrap();
            }
        }
        for cycle in 0..48u64 {
            for lane in 0..4 {
                if design.top_ports().iter().any(|p| p.name == "push") {
                    let push = (cycle + lane as u64) % 3 != 0;
                    let pop = (cycle + lane as u64) % 4 == 0;
                    let data = (cycle * 7 + lane as u64) & 255;
                    for (name, value) in
                        [("push", push as u64), ("pop", pop as u64), ("wdata", data)]
                    {
                        let v = Value::from_u64(if name == "wdata" { 8 } else { 1 }, value);
                        batch.drive_named(lane, name, v.clone()).unwrap();
                        reference[lane].drive_named(name, v).unwrap();
                    }
                } else {
                    let we = cycle % 3 != 0;
                    let re = cycle % 2 == 0;
                    for (name, width, value) in [
                        ("we", 1, we as u64),
                        ("re", 1, re as u64),
                        ("waddr", 4, (cycle + lane as u64) % 16),
                        ("raddr", 4, (cycle * 3 + lane as u64) % 16),
                        ("wdata", 8, cycle + lane as u64),
                    ] {
                        let v = Value::from_u64(width, value);
                        batch.drive_named(lane, name, v.clone()).unwrap();
                        reference[lane].drive_named(name, v).unwrap();
                    }
                }
            }
            batch.eval_comb().unwrap();
            for sim in &mut reference {
                sim.eval_comb().unwrap();
            }
            for (lane, sim) in reference.iter().enumerate() {
                for name in outputs {
                    assert_eq!(
                        batch.read_named(lane, name).unwrap(),
                        sim.read_named(name).unwrap(),
                        "cycle {cycle}, lane {lane}, {name}"
                    );
                }
            }
            batch.tick("clk").unwrap();
            for sim in &mut reference {
                sim.tick("clk").unwrap();
            }
        }
    }
}
