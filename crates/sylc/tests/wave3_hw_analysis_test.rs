use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicUsize, Ordering},
};
use syl_emit::SystemVerilogBackend;
use syl_session::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "syl-wave3-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, source: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, source).unwrap();
        path
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn emit(source: &str) -> String {
    let mut host = AnalysisHost::new();
    host.open_document(
        DocumentUri::new("untitled:syl/wave3"),
        source.into(),
        DocumentVersion::new(1),
    );
    emit_host(&mut host)
}
fn emit_host(host: &mut AnalysisHost) -> String {
    let snapshot = host.snapshot().expect("snapshot");
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    assert!(
        snapshot.semantic_diagnostics().is_empty(),
        "{:?}",
        snapshot.semantic_diagnostics()
    );
    let diagnostics = snapshot
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("elaboration");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    SystemVerilogBackend::new()
        .emit(snapshot.hwir().expect("HWIR"))
        .expect("SV")
}
fn available(tool: &str, flag: &str) -> bool {
    match Command::new(tool).arg(flag).output() {
        Ok(out) => {
            assert!(out.status.success(), "{tool} cannot run: {out:?}");
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("Skipping {tool} check: tool unavailable");
            false
        }
        Err(error) => panic!("{tool}: {error}"),
    }
}
fn success(output: Output) {
    assert!(
        output.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn lint(path: &Path, top: &str) {
    success(
        Command::new("verilator")
            .args(["--lint-only", "--sv", "-Wno-fatal", "--top-module", top])
            .arg(path)
            .output()
            .expect("Verilator lint"),
    );
}
fn synth(path: &Path, top: &str) {
    success(
        Command::new("yosys")
            .args([
                "-Q",
                "-q",
                "-p",
                &format!(
                    "read_verilog -sv {}; hierarchy -check -top {top}; proc; memory; check -assert",
                    path.display()
                ),
            ])
            .output()
            .expect("Yosys synthesis"),
    );
}
fn simulate(dir: &Scratch, source: &Path, top: &str, bench: &str) {
    let bench = dir.write(&format!("{top}_test.cpp"), bench);
    let obj = dir.0.join(format!("obj_{top}"));
    success(
        Command::new("verilator")
            .args([
                "--cc",
                "--exe",
                "--build",
                "-j",
                "2",
                "--sv",
                "-Wno-fatal",
                "--top-module",
                top,
                "-Mdir",
            ])
            .arg(&obj)
            .arg(source)
            .arg(bench)
            .args(["-o", "simulation"])
            .output()
            .expect("build simulation"),
    );
    success(
        Command::new(obj.join("simulation"))
            .output()
            .expect("run simulation"),
    );
}

#[test]
fn all_reset_combinations_inherit_type_config_and_assert_mask_polarity() {
    for (mode, polarity, edge, condition) in [
        ("Sync", "ActiveHigh", "posedge clk", "if (rst)"),
        ("Sync", "ActiveLow", "posedge clk", "if (!rst)"),
        (
            "Async",
            "ActiveHigh",
            "posedge clk or posedge rst",
            "if (rst)",
        ),
        (
            "Async",
            "ActiveLow",
            "posedge clk or negedge rst",
            "if (!rst)",
        ),
    ] {
        let source = format!(
            "cell ResetCase(clk: in Clock, rst: in Reset<{mode}, {polarity}>) -> y: Bit {{ reg q: Bit reset(rst, 0) next q := 1 y := q assert(q); }}"
        );
        let sv = emit(&source);
        assert!(sv.contains(&format!("always @({edge})")), "{sv}");
        assert!(sv.contains(condition), "{sv}");
        let mask = if polarity == "ActiveLow" {
            "rst &"
        } else {
            "(!rst) &"
        };
        assert!(sv.contains(mask), "{sv}");
    }
}
const BRIDGE: &str = r#"
@cdc_sync(a, b, clk)
cell Bridge(clk: in Clock, rst_n: in Reset<Async, ActiveLow>, a: in Bit) -> b: Bit {
    reg stage1: Bit reset(rst_n, 0, async, active_low)
    reg stage2: Bit reset(rst_n, 0)
    next stage1 := a
    next stage2 := stage1
    b := stage2
}
"#;
#[test]
fn cdc_bridge_async_reset_passes_lint_synthesis_and_cycle_simulation() {
    let sv = emit(BRIDGE);
    let dir = Scratch::new();
    let path = dir.write("bridge.sv", &sv);
    if available("yosys", "-V") {
        synth(&path, "Bridge");
    }
    if !available("verilator", "--version") {
        return;
    }
    lint(&path, "Bridge");
    simulate(
        &dir,
        &path,
        "Bridge",
        r#"
#include "VBridge.h"
#include <cassert>
int main() {
    VBridge d;
    auto tick = [&] { d.clk = 0; d.eval(); d.clk = 1; d.eval(); d.clk = 0; d.eval(); };
    d.clk = 0; d.rst_n = 1; d.a = 1; d.eval();
    tick(); tick(); assert(d.b == 1);
    d.rst_n = 0; d.eval(); assert(d.b == 0);
    d.rst_n = 1; d.eval(); assert(d.b == 0);
    tick(); assert(d.b == 0); tick(); assert(d.b == 1);
    d.a = 0; tick(); assert(d.b == 1); tick(); assert(d.b == 0);
}
"#,
    );
}

fn ram_source(name: &str, operations: &str) -> String {
    format!(
        "cell {name}<W: nat = 8, DEPTH: nat = 16, AW: nat = 4,>(clk: in Clock, we: in Bit, re: in Bit, waddr: in UInt<AW>, raddr: in UInt<AW>, data: in UInt<W>) -> y: UInt<W> {{ mem storage: [DEPTH] UInt<W> {operations} }}"
    )
}
#[test]
fn ram_variants_pass_black_box_lowering_lint_synthesis_and_cycle_simulation() {
    let verilator = available("verilator", "--version");
    let yosys = available("yosys", "-V");
    for (name, operations, distributed, single) in [
        (
            "SinglePortRam",
            "mem_write(storage, clk, we, waddr, data); mem_read_sync(storage, clk, re, waddr, y)",
            false,
            true,
        ),
        (
            "SimpleDualPortRam",
            "mem_write((storage), clk, we, waddr, data); mem_read_sync(((storage)), clk, re, raddr, y)",
            false,
            false,
        ),
        (
            "DistributedRam",
            "mem_write(((storage)), clk, we, waddr, data); let word = storage[raddr]; y := word",
            true,
            false,
        ),
    ] {
        let sv = emit(&ram_source(name, operations));
        assert!(sv.contains("storage [0:(DEPTH)-1]"), "{sv}");
        assert!(sv.contains("always @(posedge clk)"), "{sv}");
        assert!(
            sv.contains(if distributed {
                "assign y = storage[raddr];"
            } else {
                "<= storage["
            }),
            "{sv}"
        );
        let dir = Scratch::new();
        let path = dir.write("ram.sv", &sv);
        if yosys {
            synth(&path, name);
        }
        if !verilator {
            continue;
        }
        lint(&path, name);
        let bench = format!(
            r#"
#include "V{name}.h"
#include <cassert>
int main() {{
    V{name} d;
    auto tick = [&] {{ d.clk = 0; d.eval(); d.clk = 1; d.eval(); d.clk = 0; d.eval(); }};
    d.clk = 0; d.we = 1; d.re = 0; d.raddr = 0;
    for (unsigned a = 0; a != 16; ++a) {{ d.waddr = a; d.data = (a * 17 + 5) & 255; tick(); }}
    d.we = 0; d.re = 1;
    for (unsigned a = 0; a != 16; ++a) {{
        d.waddr = a; d.raddr = a; d.eval();
        if (!{distributed}) tick();
        assert(d.y == ((a * 17 + 5) & 255));
    }}
    // Disabled writes leave contents intact; disabled synchronous reads hold.
    d.re = 0; d.waddr = 3; d.raddr = 3; d.data = 99;
    auto previous = d.y; tick();
    if (!{distributed}) assert(d.y == previous);
    d.re = 1; tick(); assert(d.y == 56);
    if (!{single}) {{
        d.we = 1; d.waddr = 2; d.raddr = 4; d.data = 77; tick();
        assert(d.y == 73);
        d.we = 0; d.raddr = 2; tick(); assert(d.y == 77);
    }}
}}
"#,
            distributed = if distributed { "true" } else { "false" },
            single = if single { "true" } else { "false" }
        );
        simulate(&dir, &path, name, &bench);
    }
}

#[test]
fn cdc_attribute_edit_invalidates_cross_package_consumers() {
    let dir = Scratch::new();
    let child = "cell Bridge(clk: in Clock, a: in Bit) -> b: Bit { reg q: Bit next q := a b := q }";
    let bridge = dir.write("bridge.syl", child);
    let top = dir.write("top.syl", "use bridge.Bridge\ncell Top<A: Domain, B: Domain>(ca: in Clock<A>, ra: in Reset<A>, cb: in Clock<B>, x: in Bit) -> y: Bit { reg q: Bit reset(ra, 0) next q := x let bridge = place Bridge(clk: cb, a: q) y := bridge }");
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(dir.0.clone()));
    host.load(&[top, bridge.clone()]).expect("load packages");
    let old = host.snapshot().unwrap();
    assert!(old.diagnostics().is_empty(), "{:?}", old.diagnostics());
    let before = old
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .unwrap();
    assert!(
        before
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION")),
        "{before:?}"
    );
    let uri = DocumentUri::from_file_path(&bridge);
    host.open_document(
        uri.clone(),
        format!("@cdc_sync(a, b, clk)\n{child}"),
        DocumentVersion::new(1),
    );
    let sv = emit_host(&mut host);
    assert!(sv.contains("module Bridge"));
    host.update_document(&uri, child.to_string()).unwrap();
    let restored = host.snapshot().unwrap();
    let after = restored
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .unwrap();
    assert!(
        after
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION")),
        "{after:?}"
    );
}

#[test]
fn memory_read_lets_emit_the_same_sv_as_direct_combinational_reads() {
    let direct = emit(&ram_source(
        "Ram",
        "mem_write(storage, clk, we, waddr, data); y := storage[raddr]",
    ));
    for read in [
        "let word = storage[raddr]; y := word",
        "let word: UInt<W> = storage[raddr]; let forwarded = word; y := forwarded",
        "let word = ((storage)[raddr]); y := word",
    ] {
        let actual = emit(&ram_source(
            "Ram",
            &format!("mem_write(((storage)), clk, we, waddr, data); {read}"),
        ));
        assert_eq!(actual, direct, "{read}");
    }
}

#[test]
fn simulation_messages_run_in_both_sv_simulators_and_synthesize_away() {
    let scratch = Scratch::new();
    let sv = emit(
        r#"
        enum State {Idle,Busy,Done}
        bundle Word { flag:Bit, data:UInt<8> }
        cell Logger(clk:in Clock, state:in State, word:in Word, en:in Bit) {
            log(when:en, "seen 100% {{ok}} {state:?} {word:#X} {word.data[7:4]:#b}")
            assert(en,"missing {word.data:#04x}")
        }"#,
    );
    let design = scratch.write("design.sv", &sv);
    scratch.write(
        "bench.sv",
        r#"module bench;
        bit clk, en=1;
        logic [1:0] state=1;
        logic [8:0] word=9'h12a;
        Logger dut(.*);
        initial begin #1; clk=1; #1; $finish; end
        endmodule"#,
    );
    let run = |program: &str, args: &[&str]| {
        let output = Command::new(program)
            .args(args)
            .current_dir(&scratch.0)
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&output.stdout).into_owned();
        success(output);
        text
    };
    let expected = "seen 100% {ok} State.Busy(2'd1) { flag: 0X1, data: 0X2A } 0b0010";
    if available("iverilog", "-V") {
        run(
            "iverilog",
            &[
                "-g2012",
                "-s",
                "bench",
                "-o",
                "test.vvp",
                "design.sv",
                "bench.sv",
            ],
        );
        assert!(run("vvp", &["test.vvp"]).contains(expected));
    }
    if available("verilator", "--version") {
        run(
            "verilator",
            &[
                "--binary",
                "-j",
                "2",
                "--timing",
                "--top-module",
                "bench",
                "design.sv",
                "bench.sv",
            ],
        );
        assert!(run("./obj_dir/Vbench", &[]).contains(expected));
    }
    if available("yosys", "-V") {
        synth(&design, "Logger");
    }
}
