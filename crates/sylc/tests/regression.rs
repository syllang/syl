use std::{collections::BTreeSet, env, fs, path::Path, process::Command};
mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_hw::ParametricHwDesign;
use syl_syntax::SourceParser;

struct TestCompiler {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl TestCompiler {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
            backend: SystemVerilogBackend::new(),
        }
    }

    fn compile(&self, source: &str) -> Result<String, String> {
        self.compile_sources(&[source])
    }

    fn compile_sources(&self, sources: &[&str]) -> Result<String, String> {
        let hwir = self.compile_hwir(sources)?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }

    fn compile_hwir(&self, sources: &[&str]) -> Result<ParametricHwDesign, String> {
        let mut files = Vec::new();
        for source in sources {
            let file = SourceParser::new(source).parse_file().map_err(|errs| {
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            files.push(file);
        }
        self.middle
            .compile_files(&files)
            .map_err(|err| err.to_string())
    }
}

struct SvOutputProbe<'a> {
    source: &'a str,
}

impl<'a> SvOutputProbe<'a> {
    fn new(source: &'a str) -> Self {
        Self { source }
    }

    fn module_names(&self) -> Result<BTreeSet<String>, String> {
        let mut names = BTreeSet::new();
        let mut endmodule_count = 0usize;
        for line in self.source.lines().map(str::trim) {
            if let Some(rest) = line.strip_prefix("module ") {
                let name = self.module_name_from_header(rest)?;
                if !names.insert(name.clone()) {
                    return Err(format!("duplicate module declaration {name}"));
                }
            } else if line == "endmodule" {
                endmodule_count += 1;
            }
        }
        if names.len() != endmodule_count {
            return Err(format!(
                "module declarations ({}) do not match endmodule count ({endmodule_count})",
                names.len()
            ));
        }
        Ok(names)
    }

    fn module_name_from_header(&self, header: &str) -> Result<String, String> {
        let name = header
            .chars()
            .take_while(|ch| ch.is_ascii_alphanumeric() || *ch == '_' || *ch == '$')
            .collect::<String>();
        if name.is_empty() {
            return Err(format!("missing module name in header {header}"));
        }
        Ok(name)
    }
}
#[test]
fn cli_project_compiles_mvp_examples_from_disk_with_valid_sv_modules() {
    let workspace = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("test cannot locate workspace root for disk-based project load");
    let out_path = env::temp_dir().join(format!("sylc-mvp-{}.sv", std::process::id()));
    let _ = fs::remove_file(&out_path);

    let output = Command::new(env!("CARGO_BIN_EXE_sylc"))
        .current_dir(&workspace)
        .arg("--out")
        .arg(&out_path)
        .arg("--std-root")
        .arg(workspace.join("examples"))
        .arg(workspace.join("examples/mvp"))
        .output()
        .expect("test cannot execute sylc binary for CLI/project e2e");

    assert!(
        output.status.success(),
        "sylc CLI failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let verilog = fs::read_to_string(&out_path)
        .expect("successful sylc CLI run must write the requested SystemVerilog output");
    let _ = fs::remove_file(&out_path);
    let modules = SvOutputProbe::new(&verilog)
        .module_names()
        .expect("backend validator success should produce structurally parseable modules");

    for expected in [
        "CombAlu",
        "CombAlu32",
        "Counter",
        "CounterPair",
        "BufferedWordPipe",
        "Lane",
        "LaneArray",
    ] {
        assert!(
            modules.contains(expected),
            "missing expected MVP module {expected}; parsed modules: {modules:?}"
        );
    }
}
#[test]
fn compiles_std_and_mvp_examples() {
    let verilog = TestCompiler::new()
        .compile_sources(&[
            include_str!("../../../examples/std/stream.syl"),
            include_str!("../../../examples/std/stage.syl"),
            include_str!("../../../examples/mvp/00_comb_alu.syl"),
            include_str!("../../../examples/mvp/01_counter.syl"),
            include_str!("../../../examples/mvp/02_stream_buffer.syl"),
            include_str!("../../../examples/mvp/03_lane_array.syl"),
        ])
        .expect("checked-in Syl examples must compile through middle and Verilog backend");

    for module in [
        "CombAlu",
        "Counter",
        "CounterPair",
        "BufferedWordPipe",
        "Lane",
        "LaneArray",
    ] {
        assert!(
            verilog.contains(&format!("module {module}")),
            "missing module {module}"
        );
    }
    assert!(!verilog.contains("module stream_skid_buffer"));
    assert!(verilog.contains("in_streams_payload"));
    assert!(verilog.contains("buffered_data"));
    assert!(verilog.contains("buffered_full"));
    assert!(!verilog.contains("ignored expression"));
    assert!(!verilog.contains("compile-time condition failed"));
    assert!(!verilog.contains(".up(up)"));
    assert!(!verilog.contains(".down(down)"));
}

#[test]
fn rejects_nat_generic_as_if_condition() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad<W: Nat>() {
    if W {
    }
}
"#,
        )
        .expect_err("Nat generic must not be accepted as an if condition");

    assert!(err.to_string().contains("requires Bool condition"));
}

#[test]
fn rejects_bool_generic_as_for_bound() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad<B: Bool>() {
    for i in 0..B {
    }
}
"#,
        )
        .expect_err("Bool generic must not be accepted as a for bound");

    assert!(err.to_string().contains("requires Nat expression"));
}

#[test]
fn skips_compile_error_in_known_zero_trip_for() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
module ZeroTrip() {
    for i in 0..0 {
        compile_error("unreachable")
    }
}
"#,
        )
        .expect("zero-trip elaboration loop must not lower its body");

    assert!(!verilog.contains("$error"));
    assert!(!verilog.contains("unreachable"));
}

#[test]
fn elaborates_const_fn_call_conditions() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
fn is_one(x: Nat) -> Bool {
    return x == 1
}

module Top(y: out Bit) {
    if is_one(1) {
        y := 1
    } else {
        y := 0
    }
}
"#,
        )
        .expect("const fn calls must be evaluated by the Const MIR evaluator");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
    assert!(!verilog.contains("generate"));
}

#[test]
fn elaborates_const_fn_cfg_with_while_and_if() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
fn clog2(x: Nat) -> Nat {
    var n: Nat = 0
    var p: Nat = 1

    while p < x {
        p = p << 1
        n = n + 1
    }

    return n
}

fn choose(x: Nat) -> Nat {
    if x == 0 {
        return 7
    }

    return x
}

module Top(y: out Bit) {
    if clog2(17) == choose(5) {
        y := 1
    } else {
        y := 0
    }
}
"#,
        )
        .expect("Const MIR evaluator must execute fn CFG with while, if, assignment and return");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
    assert!(!verilog.contains("gen_if"));
}

#[test]
fn elaborates_loop_local_const_conditions() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
module Top(y0: out Bit, y1: out Bit) {
    for i in 0..2 {
        if i == 0 {
            y0 := 1
        } else {
            y1 := 1
        }
    }
}
"#,
        )
        .expect("known elaboration loop values must flow into const evaluation");

    assert!(verilog.contains("assign y0 = 1;"));
    assert!(verilog.contains("assign y1 = 1;"));
    assert!(!verilog.contains("gen_if"));
}

#[test]
fn elaborates_hardware_body_local_const_conditions() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
fn is_one(x: Nat) -> Bool {
    return x == 1
}

module Top(y: out Bit) {
    const ENABLE: Bool = is_one(1)
    if ENABLE {
        y := 1
    } else {
        y := 0
    }
}
"#,
        )
        .expect("local const bindings must enter elaboration scope");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
    assert!(!verilog.contains("gen_if"));
}

#[test]
fn rejects_duplicate_instance_arguments() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Child(a: in Bit, b: in Bit) {
}

module Top(x: in Bit) {
    let u = place Child(a: x, a: x)
}
"#,
        )
        .expect_err("duplicate named argument must be rejected");

    assert!(err.to_string().contains("duplicate argument"));
}

#[test]
fn accepts_mixed_named_and_positional_instance_arguments() {
    TestCompiler::new()
        .compile(
            r#"
module Child(a: in Bit, b: in Bit, c: in Bit) {
}

module Top(x: in Bit) {
    let u = place Child(c: x, x, b: x)
}
"#,
        )
        .expect("mixed named and positional instance arguments must resolve in formal order");
}

#[test]
fn rejects_duplicate_hardware_drivers() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad(y: out Bit) {
    y := 0
    y := 1
}
"#,
        )
        .expect_err("same place must not have two unconditional drivers");

    assert!(err.contains("duplicate hardware driver for y"));
}

#[test]
fn rejects_undriven_out_port() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad(y: out Bit) {
}
"#,
        )
        .expect_err("out ports must have a driver fact");

    assert!(err.contains("out y is not driven"));
}

#[test]
fn treats_extern_out_connection_as_driver_fact() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
extern module Child(y: out Bit)

module Top(y: out Bit) {
    let child = place Child(y: y)
}
"#,
        )
        .expect("extern output connection must drive the parent actual");

    assert!(verilog.contains(".y(y)"));
}

#[test]
fn allows_same_local_driver_names_in_different_modules() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
module A(y: out Bit) {
    y := 0
}

module B(y: out Bit) {
    y := 1
}
"#,
        )
        .expect("driver graph must scope target names by module");

    assert!(verilog.contains("module A"));
    assert!(verilog.contains("module B"));
}

#[test]
fn rejects_driving_in_scalar_port() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad(x: in Bit) {
    x := 1
}
"#,
        )
        .expect_err("in scalar ports must not be drivable");

    assert!(err.contains("x is not drivable"));
}

#[test]
fn rejects_illegal_view_field_drive() {
    let err = TestCompiler::new()
        .compile(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}

module Bad(up: in Stream<Bit>.sink) {
    up.valid := 1
}
"#,
        )
        .expect_err("sink.valid is readable only from this scope");

    assert!(err.contains("up.valid is not drivable"));
}

#[test]
fn rejects_reading_write_only_view_field() {
    let err = TestCompiler::new()
        .compile(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}

module Bad(up: in Stream<Bit>.sink, y: out Bit) {
    y := up.ready
}
"#,
        )
        .expect_err("sink.ready exposes drive capability, not read capability");

    assert!(err.contains("up.ready is not readable"));
}

#[test]
fn rejects_reading_write_only_out_port() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad(y: out Bit) {
    signal tmp: Bit := y
}
"#,
        )
        .expect_err("out scalar ports must not be readable unless a view field says so");

    assert!(err.contains("y is not readable"));
}

#[test]
fn rejects_assignment_inside_map() {
    let err = TestCompiler::new()
        .compile(
            r#"
map Bad(x: Bit) -> Bit =
    x := 1

module Top(y: out Bit) {
    y := 0
}
"#,
        )
        .expect_err("map must remain pure");

    assert!(err.contains("map expressions cannot contain assignment"));
}

#[test]
fn rejects_hardware_generator_call_inside_map() {
    let err = TestCompiler::new()
        .compile(
            r#"
cell MakeBit() -> y: Bit {
    y := 1
}

map Bad() -> Bit =
    MakeBit()

module Top(y: out Bit) {
    y := 0
}
"#,
        )
        .expect_err("map must not call cell/module generators");

    assert!(err.contains("map expressions cannot call hardware generator MakeBit"));
}

#[test]
fn rejects_const_fn_call_inside_map() {
    let err = TestCompiler::new()
        .compile(
            r#"
fn choose(x: Nat) -> Nat {
    return x
}

map Bad(x: UInt<8>) -> UInt<8> =
    choose(1)

module Top(y: out UInt<8>) {
    y := 0
}
"#,
        )
        .expect_err("map lowering must not silently treat const fn calls as map calls");

    assert!(err.contains("hardware value expressions cannot call unknown function choose"));
}

#[test]
fn rejects_plain_cell_call_in_hardware_value_expr() {
    let err = TestCompiler::new()
        .compile(
            r#"
cell MakeBit() -> y: Bit {
    y := 1
}

module Top(y: out Bit) {
    signal tmp: Bit := MakeBit()
    y := tmp
}
"#,
        )
        .expect_err("cell calls in value expressions must not become SV function calls");

    assert!(err.contains("hardware value expressions cannot call generator MakeBit"));
}

#[test]
fn rejects_plain_module_call_in_hardware_value_expr() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Child() -> y: Bit {
    y := 1
}

module Top(y: out Bit) {
    y := Child()
}
"#,
        )
        .expect_err("module calls in value expressions must not become SV function calls");

    assert!(err.contains("hardware value expressions cannot call generator Child"));
}

#[test]
fn rejects_wrong_endpoint_view_at_call_site() {
    let err = TestCompiler::new()
        .compile(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view source {
        out payload
        out valid
        in ready
    }

    view sink {
        in payload
        in valid
        out ready
    }
}

module Child(up: in Stream<Bit>.sink) {
    up.ready := 1
}

module Top(up: out Stream<Bit>.source) {
    let child = place Child(up: up)
}
"#,
        )
        .expect_err("formal sink must require readable payload/valid from actual endpoint");

    assert!(err.contains("up.payload is not readable"));
}

#[test]
fn rejects_overlapping_guarded_multi_driver_without_proof() {
    let err = TestCompiler::new()
        .compile(
            r#"
module Bad<ENABLE_A: Bool, ENABLE_B: Bool>(y: out Bit) {
    if ENABLE_A {
        y := 0
    }
    if ENABLE_B {
        y := 1
    }
}
"#,
        )
        .expect_err("independent guarded drivers must conflict unless proven exclusive");

    assert!(err.contains("duplicate hardware driver for y"));
}

#[test]
fn rejects_unknown_import_paths_in_hir() {
    let err = TestCompiler::new()
        .compile(
            r#"
package examples.bad

use examples.missing.Symbol

module Top(y: out Bit) {
    y := 0
}
"#,
        )
        .expect_err("HIR must validate explicit use targets");

    assert!(err.contains("unknown import examples.missing.Symbol"));
}
