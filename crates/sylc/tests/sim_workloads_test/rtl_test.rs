use super::simulation_dir::{SimulationDir, run, tool_available};
use std::{fmt::Write, fs, path::Path, process::Command};
use syl_sim::{HwDirection, SimDesign, Simulator};

// One text row per clock: inputs (excluding clk), outputs before the edge,
// outputs after it. Concatenation in port order preserves arbitrary bus widths.
pub fn pins(sim: &Simulator, direction: HwDirection) -> String {
    let mut bits = String::new();
    for port in sim
        .design()
        .top_ports()
        .iter()
        .filter(|p| p.direction == direction && p.name != "clk")
    {
        let v = sim.read(port.signal);
        assert!(v.is_known(), "cycle {}: {}={v}", sim.cycle(), port.name);
        bits.push_str(&v.to_binary());
    }
    bits
}

#[derive(Debug)]
pub enum Backend {
    Verilator,
    Icarus,
}

impl Backend {
    fn command(&self, work: &SimulationDir) -> Command {
        let mut cmd = match self {
            Self::Verilator => Command::new(work.0.join("obj_dir/Vtb")),
            Self::Icarus => {
                let mut cmd = Command::new("vvp");
                cmd.arg("bench.vvp");
                cmd
            }
        };
        cmd.current_dir(&work.0);
        cmd
    }
}

pub fn build(work: &SimulationDir, path: &Path, d: &SimDesign, top: &str) -> Vec<Backend> {
    run(
        Command::new(env!("CARGO_BIN_EXE_sylc"))
            .current_dir(&work.0)
            .arg("emit")
            .arg(path)
            .args(["--out", "design.sv"]),
        "sylc emit",
    );
    let mut decl = String::new();
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut in_width = 0;
    let mut out_width = 0;
    for p in d.top_ports() {
        let width = d.signal(p.signal).width;
        writeln!(decl, "logic [{}:0] {};", width - 1, p.name).unwrap();
        if p.direction == HwDirection::Out {
            outputs.push(p.name.as_str());
            out_width += width;
        } else if p.name != "clk" {
            inputs.push(p.name.as_str());
            in_width += width;
        }
    }
    let inputs = inputs.join(", ");
    let outputs = outputs.join(", ");
    let bench = format!(
        r#"module tb;
{decl}
{top} dut(.*);
logic [{in_width}-1:0] values;
logic [{out_width}-1:0] before_edge, after_edge;
int trace, cycle;
initial begin
    clk = 0;
    cycle = 0;
    trace = $fopen("trace", "r");
    while ($fscanf(trace, "%b %b %b", values, before_edge, after_edge) == 3) begin
        cycle++;
        {{{inputs}}} = values;
        #1;
        if (cycle > 1 && {{{outputs}}} !== before_edge)
            $fatal(1, "cycle %0d pre-edge: expected %h got %h", cycle, before_edge, {{{outputs}}});
        if ($test$plusargs("log_edges")) $display("EDGE %0d", cycle);
        clk = 1;
        #1;
        if ({{{outputs}}} !== after_edge)
            $fatal(1, "cycle %0d post-edge: expected %h got %h", cycle, after_edge, {{{outputs}}});
        clk = 0;
        #1;
    end
    $display("PASS %0d", cycle);
    $finish;
end
endmodule
"#
    );
    fs::write(work.0.join("tb.sv"), bench).unwrap();
    let mut backends = Vec::new();
    if tool_available("verilator") {
        run(
            Command::new("verilator").current_dir(&work.0).args([
                "--binary",
                "--timing",
                "-j",
                "2",
                "--top-module",
                "tb",
                "tb.sv",
                "design.sv",
            ]),
            "Verilator build",
        );
        backends.push(Backend::Verilator);
    } else {
        eprintln!("SKIP {top}: Verilator unavailable");
    }
    if tool_available("iverilog") && tool_available("vvp") {
        run(
            Command::new("iverilog").current_dir(&work.0).args([
                "-g2012",
                "-s",
                "tb",
                "-o",
                "bench.vvp",
                "tb.sv",
                "design.sv",
            ]),
            "Icarus build",
        );
        backends.push(Backend::Icarus);
    } else {
        eprintln!("SKIP {top}: Icarus unavailable");
    }
    backends
}

pub fn compare(work: &SimulationDir, backends: &[Backend], trace: &str, cycles: usize) {
    fs::write(work.0.join("trace"), trace).unwrap();
    for backend in backends {
        let out = run(&mut backend.command(work), "RTL comparison");
        let log = String::from_utf8(out.stdout).unwrap();
        assert!(!log.contains("assert failed"), "{backend:?}: {log}");
        assert!(
            log.lines().any(|l| l == format!("PASS {cycles}")),
            "{backend:?}: {log}"
        );
    }
}

pub fn expect_assertion(work: &SimulationDir, backends: &[Backend], trace: &str, cycle: u64) {
    fs::write(work.0.join("trace"), trace).unwrap();
    for backend in backends {
        let out = backend.command(work).arg("+log_edges").output().unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        // Icarus can continue after $error. Locate the first assertion in the edge log.
        let (before, _) = log
            .split_once("assert failed")
            .unwrap_or_else(|| panic!("{backend:?}: {log}"));
        assert_eq!(
            before
                .lines()
                .filter_map(|l| l.strip_prefix("EDGE "))
                .next_back(),
            Some(cycle.to_string().as_str()),
            "{backend:?}: {log}"
        );
        assert!(log.contains("guard_inst"), "{backend:?}: {log}");
        if matches!(backend, Backend::Verilator) {
            assert!(!out.status.success(), "Verilator missed the assertion");
        }
    }
}
