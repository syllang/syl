#[path = "sim_workloads_test/rtl_test.rs"]
mod rtl;
#[path = "support/simulation_dir_test.rs"]
mod simulation_dir;
mod support;

use simulation_dir::{SimulationDir, run};
use std::{
    fmt::Write,
    fs,
    path::{Path, PathBuf},
    process::Command,
};
use syl_sim::{HwDirection, RuntimeError, SimDesign, Simulator, Value};
use syl_span::{SourceId, Span};

struct Workload {
    dir: SimulationDir,
    path: PathBuf,
    design: SimDesign,
    backends: Vec<rtl::Backend>,
}

impl Workload {
    fn new(file: &str, top: &str) -> Self {
        let dir = SimulationDir::new(top);
        let path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/benchmarks")
            .join(file);
        run(
            Command::new(env!("CARGO_BIN_EXE_sylc"))
                .arg("check")
                .arg(&path),
            "sylc check",
        );
        let source = fs::read_to_string(&path).unwrap();
        let elaborated = support::MiddleCompiler::new()
            .output_sources(&[&source])
            .unwrap();
        let design = SimDesign::lower(&elaborated, top, &Default::default()).unwrap();
        eprintln!(
            "{top}: {} comb assignments, {} registers, {} instances, families {:?}",
            design.comb.len(),
            design.regs.len(),
            design.instances.len(),
            design
                .families
                .iter()
                .map(|f| (&f.module, f.members.len()))
                .collect::<Vec<_>>()
        );
        let backends = rtl::build(&dir, &path, &design, top);
        Self {
            dir,
            path,
            design,
            backends,
        }
    }

    fn compare(&self, cycles: usize, mut stimulus: impl FnMut(usize, &mut Simulator)) {
        let mut sim = Simulator::new(&self.design);
        drive(&mut sim, "clk", 0);
        let mut rows = String::new();
        for cycle in 1..=cycles {
            drive(&mut sim, "rst", u64::from(cycle == 1));
            stimulus(cycle, &mut sim);
            sim.eval_comb().unwrap();
            let input = rtl::pins(&sim, HwDirection::In);
            let before = if cycle == 1 {
                "0".into()
            } else {
                rtl::pins(&sim, HwDirection::Out)
            };
            sim.tick("clk").unwrap();
            let after = rtl::pins(&sim, HwDirection::Out);
            writeln!(rows, "{input} {before} {after}").unwrap();
        }
        rtl::compare(&self.dir, &self.backends, &rows, cycles);
    }
}

fn drive(sim: &mut Simulator, name: &str, n: u64) {
    let width = sim.design().signal(sim.port(name).unwrap()).width;
    sim.drive_named(name, Value::from_u64(width, n)).unwrap();
}

fn advance(state: &mut u32) -> u32 {
    *state ^= *state << 13;
    *state ^= *state >> 17;
    *state ^= *state << 5;
    *state
}

#[test]
fn independent_wide_alu_lanes_match_rtl() {
    let w = Workload::new("wide_alu_array.syl", "WideAluArray");
    let pe = w
        .design
        .families
        .iter()
        .find(|f| f.module.last().unwrap() == "WideAluPE")
        .unwrap();
    assert_eq!(pe.members.len(), 4);
    assert_eq!(w.design.regs.len(), 2 * pe.members.len());
    let mut rng = 0xf12a00bd;
    w.compare(128, |cycle, sim| {
        let words = (0..pe.members.len())
            .map(|_| Value::from_u64(32, advance(&mut rng).into()))
            .collect::<Vec<_>>();
        sim.drive_named("words", Value::concat(&words).unwrap())
            .unwrap();
        drive(
            sim,
            "ops",
            0xe4u8.rotate_left((cycle as u32 % 4) * 2).into(),
        );
    });
}

#[test]
fn serial_control_cascade_matches_rtl() {
    let w = Workload::new("deep_fsm_chain.syl", "DeepFsmChain");
    assert_eq!(w.design.assertions.len(), 64);
    let mut rng = 0x64d3ea75;
    w.compare(128, |_, sim| {
        drive(sim, "command", (advance(&mut rng) & 3).into())
    });
}

#[test]
fn competing_clients_read_back_shared_ram() {
    let w = Workload::new("shared_ram_arbiter.syl", "SharedRamArbiter");
    let [ram] = w.design.memories.as_slice() else {
        panic!("expected one shared RAM")
    };
    assert_eq!(
        (ram.depth, ram.width, ram.writes.len(), ram.sync_reads.len()),
        (16, 32, 1, 1)
    );
    let mut rng = 0x215aed3b;
    w.compare(128, |cycle, sim| {
        // Fill all addresses through port A. Then both ports repeatedly read
        // the same hot word before switching to mixed random traffic.
        let (req_a, we_a, addr_a, req_b, we_b, addr_b) = match cycle {
            1..=17 => (1, 1, cycle.saturating_sub(2), 0, 0, 0),
            18..=33 => (1, 0, 7, 1, 0, 7),
            _ => {
                let bits = advance(&mut rng) as usize;
                (
                    bits & 1,
                    (bits >> 1) & 1,
                    (bits >> 2) & 15,
                    (bits >> 6) & 1,
                    (bits >> 7) & 1,
                    (bits >> 8) & 15,
                )
            }
        };
        for (pin, val) in [
            ("req_a", req_a),
            ("we_a", we_a),
            ("addr_a", addr_a),
            ("req_b", req_b),
            ("we_b", we_b),
            ("addr_b", addr_b),
        ] {
            drive(sim, pin, val as u64);
        }
        drive(sim, "data_a", 0xfedc0000 + cycle as u64);
        drive(sim, "data_b", advance(&mut rng).into());
        if (19..=34).contains(&cycle) {
            let winner = if cycle % 2 == 1 { "b" } else { "a" };
            assert_eq!(
                sim.read_named(&format!("valid_{winner}")).unwrap().as_u64(),
                Some(1)
            );
            assert_eq!(
                sim.read_named(&format!("rsp_{winner}")).unwrap().as_u64(),
                Some(0xfedc0009)
            );
        }
    });
}

const REPLAY_SEED: u32 = 0x5eed1234;

#[test]
fn fault_sequence_matches_rtl_and_replays_with_source_location() {
    let w = Workload::new("fault_hunt_pipeline.syl", "FaultHuntPipeline");
    let mut rng = REPLAY_SEED;
    w.compare(128, |cycle, sim| {
        drive(sim, "sample", (advance(&mut rng) >> 24).into());
        drive(sim, "inject", u64::from(cycle != 20));
    });
    let mut rng = REPLAY_SEED;
    let mut sim = Simulator::new(&w.design);
    drive(&mut sim, "clk", 0);
    let mut steps = Vec::new();
    let mut fault_trace = String::new();
    for cycle in 1..=20 {
        let sample = advance(&mut rng) >> 24;
        let reset = u64::from(cycle == 1);
        drive(&mut sim, "rst", reset);
        drive(&mut sim, "inject", 1);
        drive(&mut sim, "sample", sample.into());
        steps.push(
            serde_json::json!({"drive":{"rst":reset,"inject":1,"sample":sample},"clocks":["clk"]}),
        );
        sim.eval_comb().unwrap();
        let input = rtl::pins(&sim, HwDirection::In);
        let before = if cycle == 1 {
            "0".into()
        } else {
            rtl::pins(&sim, HwDirection::Out)
        };
        let result = sim.tick("clk");
        if cycle < 20 {
            result.unwrap();
            let after = rtl::pins(&sim, HwDirection::Out);
            writeln!(fault_trace, "{input} {before} {after}").unwrap();
        } else {
            let Err(RuntimeError::Assertion(f)) = result else {
                panic!("expected assertion at edge 20")
            };
            let src = fs::read_to_string(&w.path).unwrap();
            let assertion = "assert(not trip)";
            let start = src.find(assertion).unwrap();
            assert_eq!(
                f.span,
                Span::new_in(SourceId::new(0), start, start + assertion.len())
            );
            assert_eq!(f.path, "FaultHuntPipeline.guard.assert[0]");
            assert_eq!(f.cycle, cycle);
            writeln!(fault_trace, "{input} {before} 0").unwrap();
        }
    }
    rtl::expect_assertion(&w.dir, &w.backends, &fault_trace, 20);
    fs::write(
        w.dir.0.join("stimulus.json"),
        serde_json::to_vec(&steps).unwrap(),
    )
    .unwrap();
    let out = Command::new(env!("CARGO_BIN_EXE_sylc"))
        .current_dir(&w.dir.0)
        .arg("sim")
        .arg(&w.path)
        .args([
            "--top",
            "FaultHuntPipeline",
            "--stimulus",
            "stimulus.json",
            "--vcd",
            "failure.vcd",
        ])
        .output()
        .unwrap();
    assert!(!out.status.success());
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.contains("cycle 20, FaultHuntPipeline.guard.assert[0]"),
        "{stderr}"
    );
    let vcd = fs::read_to_string(w.dir.0.join("failure.vcd")).unwrap();
    assert!(vcd.contains("$scope module guard $end"));
    assert!(vcd.contains(" trip $end"));
    assert!(vcd.contains("#57\n"));
}
