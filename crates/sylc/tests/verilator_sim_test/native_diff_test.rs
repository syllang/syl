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
            compare(&root, &path, &d, top, native_trace(&d));
        }
    }
}
fn compare(root: &Path, path: &Path, d: &SimDesign, top: &str, (trace, rows): (String, usize)) {
    let work = SimulationDir::new(top);
    let sv = work.0.join("design.sv");
    run(
        Command::new(env!("CARGO_BIN_EXE_sylc"))
            .args(["emit", "--std-root"])
            .arg(root)
            .arg(path)
            .arg("--out")
            .arg(&sv),
        "sylc emit",
    );
    fs::write(work.0.join("trace"), trace).unwrap();
    fs::write(work.0.join("bench.cpp"), bench(d, top, rows)).unwrap();
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

fn record(trace: &mut String, s: &Simulator, row: usize, fully_known: bool) {
    let d = s.design();
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
                "{} {} sample {row}: {v}",
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
}

#[test]
fn fifo_full_push_pop_and_drain_match_verilator() {
    if !tool_available("verilator") {
        return;
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/std")
        .canonicalize()
        .unwrap();
    let path = root.join("fifo.syl");
    let mut host = AnalysisHost::with_config(
        ProjectConfig::builder()
            .std_roots(vec![root.clone()])
            .build(),
    );
    let snapshot = host.load(std::slice::from_ref(&path)).unwrap();
    let top = "SyncFifo";
    let d = SimDesign::from_hwir(snapshot.hwir().unwrap(), top, &Default::default()).unwrap();
    let mut s = Simulator::new(&d);
    let clk = s.clock("clk").unwrap();
    let mut trace = String::new();
    let mut rows = 0;
    let mut count = 0;
    let mut full_attempts = 0;
    let mut full_push_pop = 0;
    let mut empty_push_pop = 0;
    let mut drives = vec![(true, false, false)];
    drives.extend([(false, true, false); 9]); // Fill, then attempt a ninth write.
    drives.push((false, true, true));
    drives.extend([(false, false, true); 7]);
    drives.extend([(false, true, true), (false, false, true)]);
    s.drive_named("clk", Value::zero(1)).unwrap();
    for (i, (reset, push, pop)) in drives.into_iter().enumerate() {
        for (p, n) in [
            ("rst", u64::from(reset)),
            ("push", push.into()),
            ("pop", pop.into()),
            ("wdata", i as u64),
        ] {
            s.drive_named(p, Value::from_u64(8, n)).unwrap();
        }
        s.eval_comb().unwrap();
        record(&mut trace, &s, rows, !reset);
        rows += 1;
        if !reset {
            assert_eq!(s.read_named("count").unwrap().as_u64(), Some(count));
            let full = s.read_named("full").unwrap().as_u64() == Some(1);
            let empty = s.read_named("empty").unwrap().as_u64() == Some(1);
            full_attempts += usize::from(full && push);
            full_push_pop += usize::from(full && push && pop);
            empty_push_pop += usize::from(empty && push && pop);
            let accept_push = push && count < 8;
            let accept_pop = pop && count > 0;
            count = count + u64::from(accept_push) - u64::from(accept_pop);
        }
        for edge in [ClockEvent::Rising(clk), ClockEvent::Falling(clk)] {
            s.step_clocks(&[edge]).unwrap();
            assert_eq!(s.read_named("count").unwrap().as_u64(), Some(count));
            record(&mut trace, &s, rows, true);
            rows += 1;
        }
    }
    assert_eq!(
        (full_attempts, full_push_pop, empty_push_pop, count),
        (2, 1, 1, 0)
    );
    compare(&root, &path, &d, top, (trace, rows));
}
fn native_trace(d: &SimDesign) -> (String, usize) {
    let mut s = Simulator::new(d);
    let mut seed = 0x71c425bdu32;
    let mut trace = String::new();
    let mut rows = 0;
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
        record(&mut trace, &s, rows, n > 16);
        rows += 1;
        let domains = (0..d.clocks.len())
            .filter(|i| d.clocks.len() == 1 || n < 3 || n % (*i as u64 + 2) == 0)
            .map(DomainId)
            .collect::<Vec<_>>();
        for edge in [ClockEvent::Rising, ClockEvent::Falling] {
            s.step_clocks(&domains.iter().copied().map(edge).collect::<Vec<_>>())
                .unwrap();
            record(&mut trace, &s, rows, n > 16);
            rows += 1;
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
