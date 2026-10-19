mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;

fn compile(source: &str) -> Result<String, String> {
    let hwir = MiddleCompiler::new().compile_sources(&[source])?;
    SystemVerilogBackend::new()
        .emit(&hwir)
        .map_err(|err| err.to_string())
}

#[test]
fn rejects_illegal_view_field_drive() {
    let err = compile(
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

cell Bad(up: in Stream<Bit>.sink) {
    up.valid := 1
}
"#,
    )
    .expect_err("sink.valid is readable only from this scope");

    assert!(err.contains("up.valid is not drivable"));
}

#[test]
fn rejects_reading_write_only_view_field() {
    let err = compile(
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

cell Bad(up: in Stream<Bit>.sink, y: out Bit) {
    y := up.ready
}
"#,
    )
    .expect_err("sink.ready exposes drive capability, not read capability");

    assert!(err.contains("up.ready is not readable"));
}

#[test]
fn rejects_reading_write_only_out_port() {
    let err = compile(
        r#"
cell Bad(y: out Bit) {
    signal tmp: Bit := y
}
"#,
    )
    .expect_err("out scalar ports must not be readable unless a view field says so");

    assert!(err.contains("y is not readable"));
}

#[test]
fn rejects_assignment_inside_map() {
    let err = compile(
        r#"
map Bad(x: Bit) -> Bit =
    x := 1

cell Top(y: out Bit) {
    y := 0
}
"#,
    )
    .expect_err("map must remain pure");

    assert!(err.contains("expected item"));
}

#[test]
fn rejects_hardware_generator_call_inside_map() {
    let err = compile(
        r#"
cell MakeBit() -> y: Bit {
    y := 1
}

map Bad() -> Bit =
    MakeBit()

cell Top(y: out Bit) {
    y := 0
}
"#,
    )
    .expect_err("map must not call cell generators");

    assert!(err.contains("map expressions cannot call hardware generator MakeBit"));
}

#[test]
fn rejects_const_fn_call_inside_map() {
    let err = compile(
        r#"
fn choose(x: nat) -> nat {
    return x
}

map Bad(x: UInt<8>) -> UInt<8> =
    choose(1)

cell Top(y: out UInt<8>) {
    y := 0
}
"#,
    )
    .expect_err("map lowering must not silently treat const fn calls as map calls");

    assert!(err.contains("hardware value expressions cannot call unknown function choose"));
}

#[test]
fn rejects_plain_cell_call_in_hardware_value_expr() {
    let err = compile(
        r#"
cell MakeBit() -> y: Bit {
    y := 1
}

cell Top(y: out Bit) {
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
    let err = compile(
        r#"
cell Child() -> y: Bit {
    y := 1
}

cell Top(y: out Bit) {
    y := Child()
}
"#,
    )
    .expect_err("cell calls in value expressions must not become SV function calls");

    assert!(err.contains("hardware value expressions cannot call generator Child"));
}

#[test]
fn rejects_wrong_endpoint_view_at_call_site() {
    let err = compile(
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

cell Child(up: in Stream<Bit>.sink) {
    up.ready := 1
}

cell Top(up: out Stream<Bit>.source) {
    let child = place Child(up: up)
}
"#,
    )
    .expect_err("formal sink must require readable payload/valid from actual endpoint");

    assert!(err.contains("up.payload is not readable"));
}

#[test]
fn rejects_overlapping_guarded_multi_driver_without_proof() {
    let err = compile(
        r#"
cell Bad<ENABLE_A: bool, ENABLE_B: bool>(y: out Bit) {
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
    let err = compile(
        r#"
use examples.missing.Symbol

cell Top(y: out Bit) {
    y := 0
}
"#,
    )
    .expect_err("HIR must validate explicit use targets");

    assert!(err.contains("unknown import examples.missing.Symbol"));
}
