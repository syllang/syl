mod support;

use support::MiddleCompiler;
use syl_elab::ElaborationOutput;
use syl_hw::{HwPlace, HwPlaceExpr, ParametricHwDesign};

struct DriverHarness {
    middle: MiddleCompiler,
}

impl DriverHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
        }
    }

    fn check(&self, source: &str) -> Result<(), String> {
        self.compile_hwir(source).map(|_| ())
    }

    fn compile_hwir(&self, source: &str) -> Result<ParametricHwDesign, String> {
        self.compile_hwir_sources(&[source])
    }

    fn compile_output(&self, source: &str) -> Result<ElaborationOutput, String> {
        self.compile_output_sources(&[source])
    }

    fn compile_hwir_sources(&self, sources: &[&str]) -> Result<ParametricHwDesign, String> {
        self.middle.compile_sources(sources)
    }

    fn compile_hwir_sources_with_paths(
        &self,
        sources: &[(Vec<String>, &str)],
    ) -> Result<ParametricHwDesign, String> {
        self.middle.compile_sources_with_paths(sources)
    }

    fn compile_output_sources(&self, sources: &[&str]) -> Result<ElaborationOutput, String> {
        self.middle.output_sources(sources)
    }
}

#[test]
fn rejects_whole_place_and_projection_multi_driver_overlap() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out UInt<2>) {
    y[0] := 1
    y := 0
}
"#,
        )
        .expect_err("whole-place drive must conflict with projection drive");

    assert!(err.contains("duplicate hardware driver for y"));
}

#[test]
fn projection_driver_fact_keeps_structured_index_expr() {
    let output = DriverHarness::new()
        .compile_output(
            r#"
cell Top(y: out UInt<2>) {
    y[0] := 1
    y[1] := 0
}
"#,
        )
        .expect("projection drive should compile");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    assert!(metadata.driver_facts().iter().any(|fact| {
        matches!(
            fact.target_place(),
            HwPlace::Index {
                base,
                index: HwPlaceExpr::Int(0),
                ..
            } if matches!(base.as_ref(), HwPlace::Object { name, .. } if name == "y")
        )
    }));
}

#[test]
fn rejects_partial_scalar_output_coverage() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out UInt<2>) {
    y[0] := 1
}
"#,
        )
        .expect_err("one driven bit must not prove a wider output is fully driven");

    assert!(err.contains("out y is not driven"));
}

#[test]
fn allows_full_scalar_output_coverage_from_bits() {
    DriverHarness::new()
        .check(
            r#"
cell Good(y: out UInt<2>) {
    y[0] := 1
    y[1] := 0
}
"#,
        )
        .expect("all static output bits are covered");
}

#[test]
fn rejects_branch_local_partial_output_coverage() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<ENABLE: Bool>(y: out UInt<2>) {
    if ENABLE {
        y[0] := 1
    } else {
        y[1] := 0
    }
}
"#,
        )
        .expect_err("each output bit must be covered on every guard path");

    assert!(err.contains("out y is not driven"));
}

#[test]
fn rejects_dynamic_index_as_output_completeness_proof() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<I: Nat>(y: out [4] Bit) {
    y[I] := 1
}
"#,
        )
        .expect_err("dynamic index coverage must not prove complete static output coverage");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_symbolic_width_projection_as_output_completeness_proof() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<W: Nat>(y: out UInt<W>) {
    y[0] := 1
}
"#,
        )
        .expect_err("symbolic output width still requires a whole-output coverage proof");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_symbolic_loop_as_scalar_output_completeness_proof() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<N: Nat>(y: out Bit) {
    for i in 0..N {
        y := 1
    }
}
"#,
        )
        .expect_err("a symbolic loop drive must not prove scalar output coverage");

    assert!(err.contains("out y is not driven"));
}

#[test]
fn rejects_cross_root_projection_as_output_coverage() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out Bit) {
    signal tmp: Bit
    tmp[0] := 1
}
"#,
        )
        .expect_err("driving a local signal must not cover an unrelated output");

    assert!(err.contains("out y is not driven"));
}

#[test]
fn rejects_out_of_bounds_output_bit_drive() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out UInt<2>) {
    y[0] := 1
    y[1] := 0
    y[3] := 1
}
"#,
        )
        .expect_err("static out-of-bounds output bit must be rejected");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_out_of_bounds_input_bit_read() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(x: in UInt<2>, y: out Bit) {
    y := x[3]
}
"#,
        )
        .expect_err("static out-of-bounds input bit read must be rejected");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_dynamic_known_width_input_index_without_bounds_proof() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<I: Nat>(x: in [4] Bit, y: out Bit) {
    y := x[I]
}
"#,
        )
        .expect_err("dynamic index into fixed-size input needs a bounds proof");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_symbolic_width_dynamic_index_without_bounds_proof() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<W: Nat, I: Nat>(x: in UInt<W>, y: out Bit) {
    y := x[I]
}
"#,
        )
        .expect_err("dynamic index into symbolic width needs a bounds proof");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_symbolic_width_literal_index_without_lower_bound_proof() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<W: Nat>(x: in UInt<W>, y: out Bit) {
    y := x[0]
}
"#,
        )
        .expect_err("literal index into symbolic width needs a non-zero width proof");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_undriven_local_signal_read() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out Bit) {
    signal tmp: Bit
    y := tmp
}
"#,
        )
        .expect_err("reading an undriven local signal must be rejected");

    assert!(err.contains("read before it is fully driven"));
}

#[test]
fn rejects_cross_root_projection_as_local_read_coverage() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out Bit) {
    signal tmp: Bit
    signal other: Bit
    other[0] := 1
    y := tmp
}
"#,
        )
        .expect_err("driving another local signal must not cover the read source");

    assert!(err.contains("read before it is fully driven"));
}

#[test]
fn rejects_local_signal_with_no_driver() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out Bit) {
    signal tmp: Bit
    y := 1
}
"#,
        )
        .expect_err("a local signal declaration without any driver must be rejected");

    assert!(err.contains("signal tmp is not driven"));
}

#[test]
fn rejects_partial_local_signal_driver() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out Bit) {
    signal tmp: UInt<2>
    tmp[0] := 1
    y := 1
}
"#,
        )
        .expect_err("one bit driver must not prove full local signal initialization");

    assert!(err.contains("signal tmp is not driven"));
}

#[test]
fn rejects_guarded_partial_local_signal_driver() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<ENABLE: Bool>(y: out Bit) {
    signal tmp: Bit
    if ENABLE {
        tmp := 1
    }
    y := 1
}
"#,
        )
        .expect_err("a local signal driver must cover every guard path");

    assert!(err.contains("signal tmp is not driven"));
}

#[test]
fn rejects_undriven_local_view_output_field() {
    let err = DriverHarness::new()
        .check(
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
}

cell Bad(y: out Bit) {
    signal tmp: Stream<Bit>.source
    tmp.valid := 1
    y := 1
}
"#,
        )
        .expect_err("local view fields owned by this scope must be fully driven");

    assert!(err.contains("signal tmp_payload is not driven"));
}

#[test]
fn allows_local_signal_read_when_driven_under_same_guard() {
    DriverHarness::new()
        .check(
            r#"
cell Maybe<ENABLE: Bool>(y: out Bit) {
    signal tmp: Bit
    if ENABLE {
        tmp := 1
        y := tmp
    } else {
        tmp := 0
        y := 0
    }
}
"#,
        )
        .expect("a read guarded by the same condition as its local drive is initialized");
}

#[test]
fn rejects_opposite_branch_drivers_from_different_sources() {
    let err = DriverHarness::new()
        .compile_hwir_sources_with_paths(&[
            (
                vec!["a".to_string()],
                r#"
cell A<E: Bool>(y: out Bit) {
    if E {
        y := 1
    }
}
"#,
            ),
            (
                vec!["b".to_string()],
                r#"
cell B<E: Bool>(y: out Bit) {
    if E {
    } else {
        y := 0
    }
}
"#,
            ),
            (
                vec!["top".to_string()],
                r#"
use a.A
use b.B

cell Top<X: Bool, Z: Bool>(y: out Bit) {
    let first = place A<X>(y: y)
    let second = place B<Z>(y: y)
}
"#,
            ),
        ])
        .map(|_| ())
        .expect_err("same-offset guards from different source files are not mutually exclusive");

    assert!(err.contains("duplicate hardware driver"));
}

#[test]
fn driver_facts_expose_object_identity() {
    let output = DriverHarness::new()
        .compile_output(
            r#"
cell Top(y: out Bit) {
    signal tmp: Bit := 1
    y := tmp
}
"#,
        )
        .expect("driver metadata should compile");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let tmp_object = metadata
        .create_facts()
        .iter()
        .find(|fact| fact.module() == "Top" && fact.name() == "tmp")
        .map(|fact| fact.object_id())
        .expect("tmp signal must have an object id");

    assert!(metadata.driver_facts().iter().any(|fact| {
        matches!(
            fact.target_place(),
            HwPlace::Object { id, name } if *id == tmp_object && name == "tmp"
        )
    }));
}

#[test]
fn rejects_parent_and_child_projection_multi_driver_overlap() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out [2] UInt<8>) {
    y[0] := 0
    y[0][0] := 1
}
"#,
        )
        .expect_err("driving an array element and one bit of the same element must conflict");

    assert!(err.contains("duplicate hardware driver for idx(part(y,0,8),0)"));
}

#[test]
fn allows_distinct_child_projection_drivers() {
    DriverHarness::new()
        .check(
            r#"
cell Good(done: out Bit) {
    signal y: [2] UInt<8>
    y[0][0] := 1
    y[0][1] := 0
    y[0][2] := 0
    y[0][3] := 0
    y[0][4] := 0
    y[0][5] := 0
    y[0][6] := 0
    y[0][7] := 0
    y[1] := 0
    done := 1
}
"#,
        )
        .expect("distinct bit projections and distinct array elements must not overlap");
}

#[test]
fn rejects_unproven_dynamic_index_multi_driver_overlap() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<N: Nat, I: Nat, J: Nat>(y: out [N] Bit) {
    y[I] := 1
    y[J] := 0
}
"#,
        )
        .expect_err("unproven dynamic sibling indices must be rejected");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn rejects_dynamic_parent_and_child_projection_overlap() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad<N: Nat, I: Nat, J: Nat>(y: out [N] UInt<8>) {
    y[I] := 0
    y[J][0] := 1
}
"#,
        )
        .expect_err("unproven dynamic parent and child projections must be rejected");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn allows_statically_distinct_index_multi_driver_projections() {
    DriverHarness::new()
        .check(
            r#"
cell Good(done: out Bit) {
    signal y: [4] Bit
    y[0] := 1
    y[1] := 0
    y[2] := 1
    y[3] := 0
    done := 1
}
"#,
        )
        .expect("distinct literal indices are statically disjoint");
}

#[test]
fn rejects_same_constant_index_multi_driver_projections() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(done: out Bit) {
    signal y: [2] Bit
    y[0] := 1
    y[0] := 0
    y[1] := 1
    done := 1
}
"#,
        )
        .expect_err("the same literal index is the same driven place");

    assert!(err.contains("duplicate hardware driver for idx(y,0)"));
}

#[test]
fn rejects_nested_parent_child_projection_overlap() {
    let err = DriverHarness::new()
        .check(
            r#"
cell Bad(y: out [2] [2] UInt<4>) {
    y[0][1] := 0
    y[0][1][2] := 1
}
"#,
        )
        .expect_err("unsupported deep parent and child projections must be rejected");

    assert!(err.contains("outside the bounds"));
}

#[test]
fn allows_deep_statically_distinct_projection_drivers() {
    DriverHarness::new()
        .check(
            r#"
cell Good(done: out Bit) {
    signal y: [2] UInt<4>
    y[0][0] := 0
    y[0][1] := 0
    y[0][2] := 0
    y[0][3] := 0
    y[1] := 1
    done := 1
}
"#,
        )
        .expect("deep literal projections with distinct segments must not overlap");
}

#[test]
fn allows_distinct_bundle_field_drivers() {
    DriverHarness::new()
        .check(
            r#"
bundle Pair {
    lo: UInt<4>,
    hi: UInt<4>,
}

cell Good(y: out Pair) {
    y.lo := 0
    y.hi := 0
}
"#,
        )
        .expect("distinct bundle fields must map to disjoint bit ranges");
}

#[test]
fn rejects_same_bundle_field_drivers() {
    let err = DriverHarness::new()
        .check(
            r#"
bundle Pair {
    lo: UInt<4>,
    hi: UInt<4>,
}

cell Bad(y: out Pair) {
    y.lo := 0
    y.lo := 1
    y.hi := 0
}
"#,
        )
        .expect_err("driving the same field twice must conflict");

    assert!(err.contains("duplicate hardware driver"));
}

#[test]
fn rejects_bundle_field_and_child_bit_overlap() {
    let err = DriverHarness::new()
        .check(
            r#"
bundle Pair {
    lo: UInt<4>,
    hi: UInt<4>,
}

cell Bad(y: out Pair) {
    y.lo := 0
    y.lo[0] := 1
}
"#,
        )
        .expect_err("driving a field and a bit inside the same field must conflict");

    assert!(err.contains("duplicate hardware driver"));
}
