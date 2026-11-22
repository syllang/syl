use super::*;
use std::fmt::Write;
use syl_session::{AnalysisHost, ProjectConfig};
use syl_sim::*;

/// Replay the exact pin values sampled by the native simulator at each
/// settle, rising edge and falling edge in an independently compiled DUT.
#[test]
fn random_port_traces_match_all_standard_cells() {
    if !tool_available("verilator") {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/std")
        .canonicalize()
        .unwrap();
    for (package, tops) in [
        ("pulse", &["EdgeDetector", "PulseStretcher"][..]),
        ("skid", &["WordSkidBuffer"]),
        ("arbiter", &["FixedPriorityArbiter4", "RoundRobinArbiter4"]),
        ("cdc", &["Sync2FF", "Sync2FFAsyncLow", "PulseSynchronizer"]),
        (
            "mem",
            &["SinglePortRam", "SimpleDualPortRam", "DistributedRam"],
        ),
        ("fifo", &["SyncFifo", "StreamSyncFifo"]),
    ] {
        let path = root.join(format!("{package}.syl"));
        let mut host = AnalysisHost::with_config(
            ProjectConfig::builder()
                .workspace_roots(vec![root.clone()])
                .std_roots(vec![root.clone()])
                .build(),
        );
        let snapshot = host.load(std::slice::from_ref(&path)).unwrap();
        for &top in tops {
            let d =
                SimDesign::from_hwir(snapshot.hwir().unwrap(), top, &Default::default()).unwrap();
            let work = SimulationDir::new(top);
            let sv = work.0.join("design.sv");
            run(
                Command::new(env!("CARGO_BIN_EXE_sylc"))
                    .args(["emit", "--std-root"])
                    .arg(&root)
                    .arg(&path)
                    .arg("--out")
                    .arg(&sv),
                "sylc emit",
            );
            let (trace, rows) = native_trace(&d);
            fs::write(work.0.join("trace"), trace).unwrap();
            fs::write(work.0.join("bench.cpp"), bench(&d, top, rows)).unwrap();
            run(
                Command::new("verilator").current_dir(&work.0).args([
                    "--cc",
                    "--exe",
                    "--build",
                    "-j",
                    "2",
                    "--sv",
                    "--top-module",
                    top,
                    "design.sv",
                    "bench.cpp",
                    "-o",
                    "compare",
                ]),
                "Verilator build",
            );
            run(
                Command::new(work.0.join("obj_dir/compare")).current_dir(&work.0),
                top,
            );
            eprintln!("{top}: {rows} matching native/Verilator samples");
        }
    }
}
fn native_trace(d: &SimDesign) -> (String, usize) {
    let mut s = Simulator::new(d);
    let mut seed = 0x71c425bdu32;
    let mut trace = String::new();
    let mut rows = 0;
    let mut record = |s: &Simulator, fully_known: bool| {
        for p in d
            .top_ports()
            .iter()
            .filter(|p| p.direction == HwDirection::In)
        {
            write!(trace, "{} ", s.read(p.signal).as_u64().unwrap()).unwrap();
        }
        for p in d
            .top_ports()
            .iter()
            .filter(|p| p.direction == HwDirection::Out)
        {
            let v = s.read(p.signal);
            if fully_known {
                assert!(
                    v.is_known(),
                    "{} {} sample {rows}: {v}",
                    d.instances[0].source.path,
                    p.name
                );
            }
            let (mut bits, mut mask) = (0u64, 0u64);
            for i in 0..v.width() {
                match v.bit(i) {
                    Logic::One => {
                        bits |= 1 << i;
                        mask |= 1 << i
                    }
                    Logic::Zero => mask |= 1 << i,
                    _ => {}
                }
            }
            write!(trace, "{bits} {mask} ").unwrap();
        }
        trace.push('\n');
        rows += 1;
    };
    for c in &d.clocks {
        s.drive(c.signal, Value::zero(1)).unwrap();
    }
    for n in 0..256 {
        for p in d.top_ports().iter().filter(|p| {
            p.direction == HwDirection::In && !d.clocks.iter().any(|c| c.signal == p.signal)
        }) {
            seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
            let mut value = u64::from(seed >> 16);
            if let Some(r) = d.resets.iter().find(|r| r.signal == p.signal) {
                value = u64::from(
                    matches!(n, 1 | 2 | 131) == (r.polarity == HwResetPolarity::ActiveHigh),
                );
            }
            // Write every RAM word, then issue a known synchronous read.
            if !d.memories.is_empty() && n <= 16 {
                match p.name.as_str() {
                    "en" | "re" => value = 1,
                    "we" => value = u64::from(n < 16),
                    "waddr" | "addr" => value = n % 16,
                    _ => {}
                }
            }
            s.drive(p.signal, Value::from_u64(d.signal(p.signal).width, value))
                .unwrap();
        }
        s.eval_comb().unwrap();
        record(&s, n > 16);
        let domains = (0..d.clocks.len())
            .filter(|i| d.clocks.len() == 1 || n < 3 || n % (*i as u64 + 2) == 0)
            .map(DomainId)
            .collect::<Vec<_>>();
        for edge in [ClockEvent::Rising, ClockEvent::Falling] {
            s.step_clocks(&domains.iter().copied().map(edge).collect::<Vec<_>>())
                .unwrap();
            record(&s, n > 16);
        }
    }
    (trace, rows)
}
fn bench(d: &SimDesign, top: &str, rows: usize) -> String {
    let mut body = String::new();
    for p in d
        .top_ports()
        .iter()
        .filter(|p| p.direction == HwDirection::In)
    {
        writeln!(
            body,
            "if (!(trace >> input)) return 2; dut.{0} = input;",
            p.name
        )
        .unwrap();
    }
    body.push_str("dut.eval();\n");
    for p in d
        .top_ports()
        .iter()
        .filter(|p| p.direction == HwDirection::Out)
    {
        writeln!(body,"if (!(trace >> expected >> mask)) return 2;\nif ((dut.{0} & mask) != expected) {{ std::cerr << \"{top}.{0} sample \" << row << \" expected \" << expected << \" got \" << +dut.{0} << \" mask \" << mask << '\\n'; return 1; }}",p.name).unwrap();
    }
    format!(
        "#include \"V{top}.h\"\n#include <fstream>\n#include <iostream>\nint main() {{V{top} dut; std::ifstream trace(\"trace\"); unsigned long long input, expected, mask; for(unsigned row=0;row<{rows};++row){{{body}}}dut.final();}}"
    )
}
