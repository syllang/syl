use syl_sema::{SemanticSession, StageOutput, TirAnalysis};
use syl_span::Span;
use syl_syntax::SourceParser;

fn check(source: &str) -> StageOutput<TirAnalysis> {
    let file = SourceParser::new(source).parse_file().expect(source);
    SemanticSession::new(&[file])
        .resolve_hir()
        .expect(source)
        .check_tir_partial()
}

fn assert_valid(source: &str) {
    let output = check(source);
    assert!(
        output.diagnostics().is_empty(),
        "{source}\n{:?}",
        output.diagnostics()
    );
}

fn assert_code(source: &str, code: &str) {
    let output = check(source);
    assert!(
        output
            .diagnostics()
            .iter()
            .any(|d| d.code.as_deref() == Some(code)),
        "{source}\n{:?}",
        output.diagnostics()
    );
}

#[test]
fn checks_software_control_flow_and_records_const_hover() {
    let source = "fn sum(n: nat) -> nat {\n    var total: nat = 0\n    let bound = n\n    const STEP: nat = 1\n    for i in 0..bound { total = total + i }\n    while total < n { total = total + STEP }\n    if total == 0 { return STEP } else { return total }\n}\n";
    let output = check(source);
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
    let stage = output.stage().unwrap();
    for text in [
        "total + i",
        "total < n",
        "total == 0",
        "return total",
        "0..bound",
    ] {
        let start = source.find(text).unwrap();
        let start = if text == "return total" {
            start + 7
        } else {
            start
        };
        let hover = stage.hover_at(Span::new(start, start + 1)).expect(text);
        assert!(
            hover.text().starts_with("Const "),
            "{text}: {}",
            hover.text()
        );
    }
}

#[test]
fn accepts_tails_recursion_forward_calls_and_hardware_typed_parameters() {
    assert_valid(
        "fn first(x: nat) -> nat { second(x) }\nfn second(x: nat) -> nat { if x == 0 { return 0 } else { return first(0) } }\nfn id(x: UInt<8>) -> UInt<8> { return x }\nfn bit(x: Bit) -> Bit { x }\nfn nothing() { return; }\n",
    );
}

#[test]
fn accepts_struct_construction_generic_fields_and_extension_calls() {
    assert_valid(
        "struct Point { x: nat, y: nat }\nstruct Box<T> { value: T }\nfn bump(this point: Point, n: nat) -> Point {\n    var result = Point { x: point.x, y: point.y }\n    result.x = result.x + n\n    return result\n}\nfn wrap(x: nat) -> Box<nat> { Box<nat> { value: x } }\nfn use_point(p: Point) -> nat {\n    let boxed = wrap(p.bump(n: 1).bump(2).x)\n    return boxed.value\n}\n",
    );
}

#[test]
fn checks_all_return_paths_and_void_returns() {
    for source in [
        "fn f() -> nat {}",
        "fn f(x: bool) -> nat { if x { return 1 } }",
        "fn f() -> nat { while true { return 1 } }",
    ] {
        assert_code(source, "E_MIDDLE_MISSING_RETURN");
    }
    for source in [
        "fn f() -> nat { return true }",
        "fn f() -> bool { 1 }",
        "fn f() -> nat { return; }",
        "fn f() { return 1 }",
        "fn f() { 1 }",
    ] {
        assert_code(source, "E_MIDDLE_RETURN_TYPE_MISMATCH");
    }
    assert_valid("fn f(x: bool) -> nat { if x { return 1 } else { return 2 } }\n");
}

#[test]
fn rejects_immutable_assignment_including_fields_and_loop_variables() {
    for source in [
        "fn f(x: nat) { x = 1 }",
        "fn f() { const x: nat = 0\n x = 1 }",
        "fn f() { let x = 0\n x = 1 }",
        "fn f() { for i in 0..2 { i = 1 } }",
        "struct P { x: nat }\nfn f(p: P) { p.x = 1 }",
    ] {
        assert_code(source, "E_MIDDLE_IMMUTABLE_ASSIGNMENT");
    }
}

#[test]
fn rejects_wrong_local_operator_condition_and_range_types() {
    for source in [
        "fn f() { var x: nat = true }",
        "fn f() { var x = 0\n x = true }",
        "fn f() { if 1 {} }",
        "fn f() { while 1 {} }",
        "fn f() -> nat { true + 1 }",
        "fn f() -> bool { !1 }",
        "fn f() -> bool { 1 && true }",
        "fn f() -> bool { 1 < false }",
    ] {
        assert_code(source, "E_MIDDLE_TYPE_MISMATCH");
    }
    assert_code(
        "fn f() { for i in true..false {} }",
        "E_MIDDLE_REQUIRES_NAT",
    );
}

#[test]
fn rejects_hardware_statements_expressions_and_calls() {
    use syl_syntax::{Expr, Item, Stmt};
    for kind in ["signal", "reg", "drive", "next"] {
        let mut ast = SourceParser::new("fn f() { var x: Bit = 0 }")
            .parse_file()
            .unwrap();
        let span = Span::new(24, 25);
        let value = Expr::Int(1, span);
        let stmt = match kind {
            "signal" => Stmt::Signal {
                name: "s".into(),
                ty: None,
                value: Some(value),
                span,
            },
            "reg" => Stmt::Reg {
                name: "r".into(),
                ty: None,
                reset: None,
                span,
            },
            "drive" => Stmt::Drive {
                target: Expr::Ident("x".into(), span),
                value,
                span,
            },
            _ => Stmt::Next {
                name: "x".into(),
                value,
                span,
            },
        };
        let Item::Fn(item) = &mut ast.items[0] else {
            unreachable!()
        };
        item.body.stmts.push(stmt);
        let output = SemanticSession::new(&[ast])
            .resolve_hir()
            .unwrap()
            .check_tir_partial();
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_HARDWARE_STMT_IN_FN")),
            "{kind}: {:?}",
            output.diagnostics()
        );
    }
    for source in [
        "map m(x: Bit) -> Bit = x\nfn f(x: Bit) -> Bit { m(x) }",
        "fn f(x: Bit) -> Bit { x and x }",
        "fn f() -> Bit { not 1 }",
    ] {
        assert_code(source, "E_MIDDLE_INVALID_ELAB_EXPR");
    }
}

#[test]
fn checks_named_positional_and_missing_call_arguments() {
    assert_valid("fn add(x: nat, y: nat) -> nat { x + y }\nfn f() -> nat { add(y: 1, x: 2) }\n");
    for source in [
        "fn f(x: nat) {}\nfn g() { f(true) }",
        "fn f(x: nat) {}\nfn g() { f() }",
    ] {
        assert_code(source, "E_MIDDLE_TYPE_MISMATCH");
    }
    assert_code(
        "fn f(x: nat) {}\nfn g() { f(1, 2) }",
        "E_MIDDLE_TOO_MANY_POSITIONAL_ARGS",
    );
    assert_code(
        "fn f(x: nat) {}\nfn g() { f(x: 1, x: 2) }",
        "E_MIDDLE_DUPLICATE_ARGUMENT",
    );
    assert_code(
        "fn f(x: nat) {}\nfn g() { f(y: 1) }",
        "E_MIDDLE_UNKNOWN_PARAMETER",
    );
}

#[test]
fn checks_aggregate_fields_and_types() {
    assert_code(
        "struct P { x: nat }\nfn f() -> P { P { x: true } }",
        "E_MIDDLE_TYPE_MISMATCH",
    );
    assert_code(
        "struct P { x: nat, y: nat }\nfn f() -> P { P { x: 1 } }",
        "E_MIDDLE_MISSING_AGGREGATE_FIELD",
    );
    assert_code(
        "struct P { x: nat }\nfn f() -> P { P { y: 1 } }",
        "E_MIDDLE_UNKNOWN_AGGREGATE_FIELD",
    );
}

#[test]
fn checks_clog2_and_preserves_user_shadowing() {
    assert_valid("fn f(n: nat) -> nat { clog2(n) }\n");
    assert_code("fn f() -> nat { clog2(true) }", "E_MIDDLE_TYPE_MISMATCH");
    assert_code("fn f() -> nat { clog2() }", "E_MIDDLE_TYPE_MISMATCH");
    assert_valid("fn clog2(n: bool) -> bool { n }\nfn f() -> bool { clog2(true) }");
}

#[test]
fn checks_software_match_values_and_hardware_call_targets() {
    assert_valid("fn f(x: Bit) -> Bit { match x { 0 => 1, _ => x } }");
    assert_valid("fn f(x: bool) -> nat { return match x { true => 1, false => 2 } }");
    assert_valid("enum State { Idle, Busy }\nfn state() -> State { State.Idle }");
    assert_code(
        "cell Hardware() {}\nfn f() { Hardware() }",
        "E_MIDDLE_INVALID_ELAB_EXPR",
    );
}

#[test]
fn rejects_void_calls_used_as_values_and_accepts_void_statements() {
    assert_valid("fn noop() {}\nfn f() { noop() }");
    assert_code(
        "fn noop() {}\nfn f() -> nat { noop() }",
        "E_MIDDLE_RETURN_TYPE_MISMATCH",
    );
    assert_code(
        "fn noop() {}\nfn f() { let x = noop() }",
        "E_MIDDLE_TYPE_MISMATCH",
    );
    assert_code(
        "fn noop() {}\nfn consume(x: nat) {}\nfn f() { consume(noop()) }",
        "E_MIDDLE_TYPE_MISMATCH",
    );
}

#[test]
fn distinguishes_void_tail_blocks_from_blocks_that_return_on_every_path() {
    for source in [
        "fn f() -> nat { {} }",
        "fn f(x: bool) -> nat { { if x { return 1 } } }",
        "fn f(x: bool) -> nat { (({ if x { return 1 } })) }",
        "fn f() -> nat { { { } } }",
    ] {
        assert_code(source, "E_MIDDLE_MISSING_RETURN");
    }
    for source in [
        "fn f() -> nat { { return 1 } }",
        "fn f(x: bool) -> nat { { if x { return 1 } else { return 2 } } }",
        "fn f() -> nat { (({ return 1 })) }",
        "fn f() -> nat { { { return 1 } } }",
        "fn f() -> nat { { 1 } }",
        "fn f() { {} }",
    ] {
        assert_valid(source);
    }
    for source in ["fn f() { let x = {} }", "fn f() { let x: nat = {} }"] {
        assert_code(source, "E_MIDDLE_TYPE_MISMATCH");
    }
    assert_code(
        "fn f() -> nat { { true } }",
        "E_MIDDLE_RETURN_TYPE_MISMATCH",
    );
}
