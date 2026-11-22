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
