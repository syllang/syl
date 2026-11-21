use std::{env, fs, path::Path, process::Command};
mod support;

use support::{MiddleCompiler, SvOutputProbe};
use syl_emit::SystemVerilogBackend;

macro_rules! path {
    ($($part:literal),+ $(,)?) => {
        vec![$($part.to_string()),+]
    };
}

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
        let hwir = self.middle.compile_sources(sources)?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }

    fn compile_sources_with_paths(
        &self,
        sources: &[(Vec<String>, &str)],
    ) -> Result<String, String> {
        let hwir = self.middle.compile_sources_with_paths(sources)?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
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
        .arg(workspace.join("examples/std"))
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
        .compile_sources_with_paths(&[
            (
                path!("std", "stream"),
                include_str!("../../../examples/std/stream.syl"),
            ),
            (
                path!("std", "stage"),
                include_str!("../../../examples/std/stage.syl"),
            ),
            (
                path!("examples", "mvp", "comb_alu"),
                include_str!("../../../examples/mvp/comb_alu.syl"),
            ),
            (
                path!("examples", "mvp", "counter"),
                include_str!("../../../examples/mvp/counter.syl"),
            ),
            (
                path!("examples", "mvp", "stream_buffer"),
                include_str!("../../../examples/mvp/stream_buffer.syl"),
            ),
            (
                path!("examples", "mvp", "lane_array"),
                include_str!("../../../examples/mvp/lane_array.syl"),
            ),
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
    // stream_skid_buffer is a cell that now produces a standalone module.
    assert!(verilog.contains("module stream_skid_buffer"));
    assert!(verilog.contains("in_streams_payload"));
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
cell Bad<W: nat>() {
    if W {
    }
}
"#,
        )
        .expect_err("nat generic must not be accepted as an if condition");

    assert!(err.to_string().contains("requires bool condition"));
}

#[test]
fn rejects_bool_generic_as_for_bound() {
    let err = TestCompiler::new()
        .compile(
            r#"
cell Bad<B: bool>() {
    for i in 0..B {
    }
}
"#,
        )
        .expect_err("bool generic must not be accepted as a for bound");

    assert!(err.to_string().contains("requires nat expression"));
}

#[test]
fn skips_compile_error_in_known_zero_trip_for() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
cell ZeroTrip() {
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
fn lowers_error_stmt_to_runtime_sv_error() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
cell Top(y: out Bit) {
    compile_error("runtime path reached")
    y := 0
}
"#,
        )
        .expect("error statement must lower into runtime SystemVerilog");

    assert!(verilog.contains("initial begin"));
    assert!(verilog.contains("$error(\"runtime path reached\");"));
    assert!(verilog.contains("assign y = 0;"));
}

#[test]
fn user_defined_error_map_is_not_captured_as_runtime_effect() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
map error(x: Bit) -> Bit =
    x

cell Top(x: in Bit, y: out Bit) {
    y := error(x)
}
"#,
        )
        .expect("user-defined error map must remain callable as an ordinary hardware value");

    assert!(verilog.contains("assign y = x;"));
    assert!(!verilog.contains("$error("));
}

#[test]
fn lowers_assert_stmt_to_clocked_runtime_sv_error() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
cell Top(clk: in Clock, x: in Bit, y: out Bit) {
    assert(x)
    y := x
}
"#,
        )
        .expect("assert statement must lower into clocked SystemVerilog");

    assert!(verilog.contains("always @(posedge clk) begin"));
    assert!(verilog.contains("if ((!x)) $error(\"assert failed\");"));
    assert!(verilog.contains("assign y = x;"));
}

#[test]
fn std_assert_helpers_remain_loadable() {
    let verilog = TestCompiler::new()
        .compile_sources_with_paths(&[
            (
                path!("std", "vendor"),
                include_str!("../../../examples/std/vendor.syl"),
            ),
            (
                path!("std", "stream"),
                include_str!("../../../examples/std/stream.syl"),
            ),
            (
                path!("std", "assert"),
                include_str!("../../../examples/std/assert.syl"),
            ),
            (
                path!("examples", "assert_helper_user"),
                r#"
use std.assert.assert_pass

cell Top(x: in Bit, y: out Bit) {
    y := assert_pass(x)
}
"#,
            ),
        ])
        .expect("existing std assert helper definitions must remain loadable");

    assert!(verilog.contains("module Top"));
}

#[test]
fn elaborates_const_fn_call_conditions() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
fn is_one(x: nat) -> bool {
    return x == 1
}

cell Top(y: out Bit) {
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
fn clog2(x: nat) -> nat {
    var n: nat = 0
    var p: nat = 1

    while p < x {
        p = p << 1
        n = n + 1
    }

    return n
}

fn choose(x: nat) -> nat {
    if x == 0 {
        return 7
    }

    return x
}

cell Top(y: out Bit) {
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
fn symbolic_struct_const_fn_conditions_do_not_reach_backend_as_calls() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
struct Config {
    enabled: bool,
}

fn choose(cfg: Config) -> bool {
    return cfg.enabled
}

cell Top<ENABLE: bool>(a: in Bit, b: in Bit, y: out Bit) {
    var cfg = Config { enabled: false }

    if ENABLE {
        cfg.enabled = true
    }

    if choose(cfg) {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("symbolic const-fn conditions should elaborate into backend-safe generate logic");

    assert!(!verilog.contains("choose("));
    assert!(verilog.contains("generate"));
    assert!(verilog.contains("assign y = b;"));
    assert!(verilog.contains("assign y = a;"));
}

#[test]
fn elaborates_loop_local_const_conditions() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
cell Top(y0: out Bit, y1: out Bit) {
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
fn is_one(x: nat) -> bool {
    return x == 1
}

cell Top(y: out Bit) {
    const ENABLE: bool = is_one(1)
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
fn elaborates_struct_const_aggregate_field_access() {
    let verilog = TestCompiler::new()
        .compile_sources_with_paths(&[
            (
                path!("alpha"),
                r#"
struct Config {
    width: nat
}
"#,
            ),
            (
                path!("beta"),
                r#"
struct Config {
    enabled: bool
}
"#,
            ),
            (
                path!("app"),
                r#"
use beta.Config;

const DEFAULT: Config = Config { enabled: true }

cell Top(y: out Bit) {
    if DEFAULT.enabled {
        y := 1
    } else {
        y := 0
    }
}
"#,
            ),
        ])
        .expect("software struct const aggregates must elaborate through field access");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
    assert!(!verilog.contains("gen_if"));
}

#[test]
fn rejects_duplicate_instance_arguments() {
    let err = TestCompiler::new()
        .compile(
            r#"
cell Child(a: in Bit, b: in Bit) {
}

cell Top(x: in Bit) {
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
cell Child(a: in Bit, b: in Bit, c: in Bit) {
}

cell Top(x: in Bit) {
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
cell Bad(y: out Bit) {
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
cell Bad(y: out Bit) {
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
extern cell Child(y: out Bit)

cell Top(y: out Bit) {
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
cell A(y: out Bit) {
    y := 0
}

cell B(y: out Bit) {
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
cell Bad(x: in Bit) {
    x := 1
}
"#,
        )
        .expect_err("in scalar ports must not be drivable");

    assert!(err.contains("x is not drivable"));
}
