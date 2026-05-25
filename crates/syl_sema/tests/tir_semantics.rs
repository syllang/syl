mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_syntax::SourceParser;

struct SemanticHarness {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl SemanticHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
            backend: SystemVerilogBackend::new(),
        }
    }

    fn compile(&self, source: &str) -> Result<String, String> {
        let file = SourceParser::new(source).parse_file().map_err(|errs| {
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let hwir = self
            .middle
            .compile_files(&[file])
            .map_err(|err| err.to_string())?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }
}

#[test]
fn rejects_unknown_port_type_before_backend_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
module Bad(x: in Missing) {
}
"#,
        )
        .expect_err("unknown types must not lower into synthetic Verilog widths");

    assert!(err.contains("unknown type Missing"));
}

#[test]
fn extension_map_method_lowers_as_receiver_call() {
    let sv = SemanticHarness::new()
        .compile(
            r#"
interface Stage<T> {
    payload: T
    valid: Bit
    ready: Bit
    cancel: Bit

    view tap {
        in payload
        in valid
        in ready
        in cancel
    }
}

map fire<T>(this stage: Stage<T>.tap) -> Bit =
    stage.valid and stage.ready and not stage.cancel

module Top(stage: in Stage<Bit>.tap, y: out Bit) {
    y := stage.fire()
}
"#,
        )
        .expect("extension map method must compile");

    assert!(sv.contains("assign y ="));
}

#[test]
fn rejects_unknown_bundle_field_type_before_width_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
bundle BadBundle {
    payload: Missing
}

module Bad(y: out BadBundle) {
    y.payload := 0
}
"#,
        )
        .expect_err("unknown bundle fields must not lower into synthetic Verilog widths");

    assert!(err.contains("unknown type Missing"));
}

#[test]
fn rejects_bool_literal_in_hardware_value_expr() {
    let err = SemanticHarness::new()
        .compile(
            r#"
module Bad(y: out Bit) {
    y := true
}
"#,
        )
        .expect_err("Bool is not a hardware Bit literal");

    assert!(err.contains("Bool is a const/proposition type"));
}

#[test]
fn rejects_proposition_operators_in_hardware_value_expr() {
    let err = SemanticHarness::new()
        .compile(
            r#"
module Bad(x: in Bit, y: out Bit) {
    y := !x == 0
}
"#,
        )
        .expect_err("hardware values must use word operators");

    assert!(err.contains("const/proposition operator"));
}

#[test]
fn rejects_select_without_default_before_eir_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
module Bad(sel: in Bit, y: out Bit) {
    y := select priority {
        sel => 1,
    }
}
"#,
        )
        .expect_err("select without default must not become an unsupported EIR expression");

    assert!(err.contains("select expression requires a default arm"));
}

#[test]
fn rejects_incomplete_aggregate_before_eir_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
bundle Pair {
    a: Bit,
    b: Bit,
}

module Bad(y: out Pair) {
    y := Pair { a: 1 }
}
"#,
        )
        .expect_err("aggregate completeness belongs in TIR, not EIR unsupported nodes");

    assert!(err.contains("aggregate field b is missing for Pair"));
}

#[test]
fn rejects_unknown_aggregate_field_before_eir_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
bundle Single {
    a: Bit,
}

module Bad(y: out Single) {
    y := Single { a: 1, b: 0 }
}
"#,
        )
        .expect_err("aggregate field names must be checked before EIR");

    assert!(err.contains("aggregate field b does not exist on Single"));
}

#[test]
fn rejects_empty_match_before_eir_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
module Bad(sel: in Bit, y: out Bit) {
    y := match sel {}
}
"#,
        )
        .expect_err("empty match must not become an unsupported EIR expression");

    assert!(err.contains("match expression requires at least one arm"));
}

#[test]
fn rejects_empty_match_in_map_before_eir_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
map bad(sel: Bit) -> Bit =
    match sel {}

module Bad(sel: in Bit, y: out Bit) {
    y := bad(sel)
}
"#,
        )
        .expect_err("empty map match must not become an unsupported EIR expression");

    assert!(err.contains("match expression requires at least one arm"));
}

#[test]
fn rejects_bool_match_pattern_before_eir_lowering() {
    let err = SemanticHarness::new()
        .compile(
            r#"
module Bad(sel: in Bit, y: out Bit) {
    y := match sel {
        true => 1,
        default => 0,
    }
}
"#,
        )
        .expect_err("Bool match patterns must not be treated as hardware Bit patterns");

    assert!(err.contains("Bool is a const/proposition type"));
}

#[test]
fn map_match_int_pattern_lowers_to_guarded_mux_not_fallback() {
    let verilog = SemanticHarness::new()
        .compile(
            r#"
map choose(sel: UInt<2>) -> UInt<4> =
    match sel {
        0 => 1,
        1 => 2,
        default => 3,
    }

module Good(sel: in UInt<2>, y: out UInt<4>) {
    y := choose(sel)
}
"#,
        )
        .expect("integer match patterns should lower as comparisons");

    assert!(verilog.contains("sel == 0"));
    assert!(verilog.contains("sel == 1"));
}

#[test]
fn preserves_unique_select_mode_through_backend_ir() {
    let verilog = SemanticHarness::new()
        .compile(
            r#"
module Good(sel: in Bit, y: out Bit) {
    y := select unique {
        sel => 1,
        default => 0,
    }
}
"#,
        )
        .expect("unique select should remain legal hardware");

    assert!(verilog.contains("/* unique */"));
}
