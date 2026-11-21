use syl_sema::{SemanticSession, SemanticSourceFile, signature_stub_file};
use syl_syntax::SourceParser;

fn diagnostics(source: &str) -> Vec<syl_span::Diagnostic> {
    let ast = SourceParser::new(source).parse_file().expect(source);
    let output = SemanticSession::new(&[ast]).check();
    output.diagnostics().to_vec()
}

#[test]
fn accepts_hardware_literals_aggregates_branches_and_polymorphic_builtins() {
    for source in [
        "map one() -> Bit = 1",
        "map one() -> UInt<8> = (0)",
        "map one<W: nat>() -> UInt<W> = 0",
        "bundle B { x: Bit }\nmap one() -> B = B { x: 1 }",
        "map one(x: Bit) -> Bit = match x { 0 => 1, _ => 0 }",
        "map one(x: Bit) -> Bit = match x { 0 => x, _ => 0 }",
        "map one(x: Bit) -> Bit = select { x => 1, default => 0 }",
        "map one() -> UInt<8> = zero()",
        "map one() -> UInt<8> = z()",
        "map id<W: nat>(x: UInt<W>) -> UInt<W> = x\nmap pass<W: nat>(x: UInt<W>) -> UInt<W> = id<W>(x)",
        "const W: nat = 8\nmap one(x: UInt<W>) -> UInt<8> = x",
        "const W: nat = K\nconst K: nat = 8\nmap one(x: UInt<W>) -> UInt<8> = x",
        "map pass(x: [2]UInt<8>) -> [2]UInt<8> = x",
        "const N: nat = 2\nmap pass(x: [N]UInt<8>) -> [2]UInt<8> = x",
        "map pass<W: nat>(x: UInt<W>) -> UInt<W> = x xor x",
        "map pass<W: nat>(x: UInt<W>) -> UInt<W> = x and x",
        "map pass<W: nat>(x: UInt<W>) -> UInt<W> = x or x",
    ] {
        assert!(
            diagnostics(source).is_empty(),
            "{source}\n{:?}",
            diagnostics(source)
        );
    }
}

#[test]
fn rejects_return_family_width_named_and_array_mismatches() {
    for source in [
        "map bad(x: Bit) -> UInt<8> = x",
        "map bad(x: UInt<8>) -> UInt<16> = x",
        "map bad(x: UInt<8>) -> SInt<8> = x",
        "map bad(x: UInt<8>) -> Bits<8> = x",
        "bundle A { x: Bit }\nbundle B { x: Bit }\nmap bad(x: A) -> B = x",
        "map bad() -> bool = 1",
        "const W: nat = 8\nmap bad(x: UInt<W>) -> UInt<16> = x",
        "map bad<A: nat, B: nat>(x: UInt<A>) -> UInt<B> = x",
        "map bad(x: [2]UInt<8>) -> [3]UInt<8> = x",
        "map bad(x: [2]UInt<8>) -> [2]UInt<16> = x",
        "bundle B<T> { x: T }\nmap bad(x: B<UInt<8>>) -> B<UInt<16>> = x",
    ] {
        let diagnostics = diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_RETURN_TYPE_MISMATCH")),
            "{source}\n{diagnostics:?}"
        );
    }
}

#[test]
fn rejects_inconsistent_match_and_select_arms_in_maps_and_cells() {
    for source in [
        "map bad(x: Bit, a: UInt<8>, b: UInt<16>) -> UInt<8> = match x { 0 => a, _ => b }",
        "map bad(x: Bit, a: UInt<8>, b: UInt<16>) -> UInt<8> = select { x => a, default => b }",
        "cell Bad(x: in Bit, a: in UInt<8>, b: in UInt<16>, y: out UInt<8>) { y := match x { 0 => a, _ => b } }",
        "cell Bad(x: in Bit, a: in UInt<8>, b: in UInt<16>, y: out UInt<8>) { y := select { x => a, default => b } }",
    ] {
        let diagnostics = diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_TYPE_MISMATCH")),
            "{source}\n{diagnostics:?}"
        );
    }
}

#[test]
fn signature_stubs_skip_body_checks_while_real_empty_functions_fail() {
    let library = SourceParser::new("fn id(x: Bit) -> Bit { x }\nmap same(x: Bit) -> Bit = x")
        .parse_file()
        .unwrap();
    let stub = signature_stub_file(&library);
    let consumer = SourceParser::new("use lib.id\nuse lib.same\nfn first(x: Bit) -> Bit { id(x) }\nmap second(x: Bit) -> Bit = same(x)").parse_file().unwrap();
    let sources = vec![
        SemanticSourceFile::new(vec!["lib".into()], &stub),
        SemanticSourceFile::new(vec!["app".into()], &consumer),
    ];
    let hir = SemanticSession::new_sources(sources).resolve_hir().unwrap();
    assert!(hir.check_tir_partial().diagnostics().is_empty());
    assert!(
        diagnostics("fn id() -> Bit {}")
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_MISSING_RETURN"))
    );
}

#[test]
fn nested_literal_branches_keep_contextual_hardware_types_in_both_orders() {
    for ty in ["Bit", "UInt<8>", "SInt<8>", "Bits<8>"] {
        for literal in [
            "match s { 0 => 0, _ => 1 }",
            "select { s => 0, default => 1 }",
            "match s { 0 => select { s => 0, default => 1 }, _ => (0) }",
            "select { s => match s { 0 => 0, _ => 1 }, default => (0) }",
        ] {
            for body in [
                format!("match s {{ 0 => {literal}, _ => x }}"),
                format!("match s {{ 0 => x, _ => {literal} }}"),
                format!("select {{ s => {literal}, default => x }}"),
                format!("select {{ s => x, default => {literal} }}"),
            ] {
                let source = format!("map f(s: Bit, x: {ty}) -> {ty} = {body}");
                let output = diagnostics(&source);
                assert!(output.is_empty(), "{source}\n{output:?}");
            }
        }
    }
    for body in [
        "match s { 0 => match s { 0 => x, _ => 0 }, _ => y }",
        "select { s => select { s => x, default => 0 }, default => y }",
    ] {
        let source = format!("map f(s: Bit, x: UInt<8>, y: UInt<16>) -> UInt<8> = {body}");
        let output = diagnostics(&source);
        assert!(
            output
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_TYPE_MISMATCH")),
            "{source}\n{output:?}"
        );
    }
}

#[test]
fn nested_zero_and_high_z_branches_keep_contextual_hardware_types() {
    for ty in ["Bit", "UInt<W>", "SInt<W>", "Bits<W>"] {
        for builtin in ["zero", "z"] {
            for literal in [
                format!("match s {{ 0 => {builtin}(), _ => 0 }}"),
                format!("match s {{ 0 => 0, _ => ({builtin}()) }}"),
                format!("select {{ s => {builtin}(), default => 0 }}"),
                format!("select {{ s => 0, default => ({builtin}()) }}"),
                format!("select {{ s => match s {{ 0 => {builtin}(), _ => 0 }}, default => 0 }}"),
            ] {
                for body in [
                    format!("match s {{ 0 => {literal}, _ => x }}"),
                    format!("match s {{ 0 => x, _ => {literal} }}"),
                    format!("select {{ s => {literal}, default => x }}"),
                    format!("select {{ s => x, default => {literal} }}"),
                ] {
                    let source = format!("map f<W: nat>(s: Bit, x: {ty}) -> {ty} = {body}");
                    let output = diagnostics(&source);
                    assert!(output.is_empty(), "{source}\n{output:?}");
                }
            }
        }
    }
}

#[test]
fn contextual_builtin_branches_preserve_shadowing_and_typed_mismatches() {
    for builtin in ["zero", "z"] {
        for (source, code) in [
            (
                format!(
                    "map {builtin}() -> nat = 0\nmap f(s: Bit, x: UInt<8>) -> UInt<8> = match s {{ 0 => match s {{ 0 => {builtin}(), _ => 0 }}, _ => x }}"
                ),
                "E_MIDDLE_TYPE_MISMATCH",
            ),
            (
                format!(
                    "map f(s: Bit, x: UInt<8>, y: UInt<16>) -> UInt<8> = select {{ s => match s {{ 0 => {builtin}(), _ => x }}, default => y }}"
                ),
                "E_MIDDLE_TYPE_MISMATCH",
            ),
            (
                format!(
                    "map f(s: Bit, x: UInt<8>) -> UInt<8> = match s {{ 0 => select {{ s => {builtin}(), default => true }}, _ => x }}"
                ),
                "E_MIDDLE_BOOL_IN_HARDWARE_VALUE",
            ),
        ] {
            let output = diagnostics(&source);
            assert!(
                output.iter().any(|d| d.code.as_deref() == Some(code)),
                "{source}\n{output:?}"
            );
        }
    }
}

#[test]
fn constant_nats_take_hardware_return_and_branch_types_in_both_orders() {
    for ty in ["Bit", "UInt<W>", "SInt<W>", "Bits<W>"] {
        for value in ["N", "C", "(N + 1)", "C * 2", "C + N", "1 + 2"] {
            for body in [
                value.to_string(),
                format!("match s {{ 0 => {value}, _ => x }}"),
                format!("match s {{ 0 => x, _ => {value} }}"),
                format!("select {{ s => {value}, default => x }}"),
                format!("select {{ s => x, default => {value} }}"),
                format!("select {{ s => match s {{ 0 => {value}, _ => 0 }}, default => x }}"),
                format!("match s {{ 0 => x, _ => select {{ s => {value}, default => 0 }} }}"),
            ] {
                let source = format!(
                    "const C: nat = 1\nmap f<N: nat, W: nat>(s: Bit, x: {ty}) -> {ty} = {body}"
                );
                let ast = SourceParser::new(&source).parse_file().expect(&source);
                let output = SemanticSession::new(&[ast])
                    .resolve_hir()
                    .expect(&source)
                    .check_tir_partial();
                assert!(
                    output.diagnostics().is_empty(),
                    "{source}\n{:?}",
                    output.diagnostics()
                );
                if body != value {
                    let start = source.len() - body.len();
                    let hover = output
                        .stage()
                        .unwrap()
                        .hover_at(syl_span::Span::new(start, start + body.len()))
                        .expect(&source);
                    assert_eq!(hover.text(), format!("Comb {ty}"), "{source}");
                }
            }
        }
    }
}

#[test]
fn constant_nats_work_in_hardware_cells_and_pulse_countdown() {
    for source in [
        "cell Pulse<CYCLES: nat = 4, CNT_W: nat = 3, D: Domain>(clk: in Clock<D>, rst: in Reset<D>, trigger: in Bit) {\n    reg remaining: UInt<CNT_W> reset(rst, 0)\n    next remaining := select priority { trigger => CYCLES, remaining eq 0 => 0, default => remaining - 1 }\n}",
        "const C: nat = 4\ncell F<W: nat>(s: in Bit, x: in UInt<W>, y: out UInt<W>) { y := match s { 0 => C + 1, _ => x } }",
        "const C = 4\ncell F<W: nat>(s: in Bit, x: in UInt<W>, y: out UInt<W>) { y := select { s => C + 1, default => x } }",
    ] {
        let output = diagnostics(source);
        assert!(output.is_empty(), "{source}\n{output:?}");
    }
}

#[test]
fn contextual_nats_preserve_runtime_nat_bool_family_and_width_mismatches() {
    for (source, code) in [
        (
            "map bad(n: nat) -> UInt<8> = n",
            "E_MIDDLE_RETURN_TYPE_MISMATCH",
        ),
        (
            "map bad(s: Bit, n: nat, x: UInt<8>) -> UInt<8> = select { s => n, default => x }",
            "E_MIDDLE_TYPE_MISMATCH",
        ),
        (
            "map bad(n: nat) -> UInt<8> = n + 1",
            "E_MIDDLE_RETURN_TYPE_MISMATCH",
        ),
        (
            "map bad<N: nat>(x: UInt<8>) -> UInt<8> = N + x",
            "E_MIDDLE_RETURN_TYPE_MISMATCH",
        ),
        (
            "const C: nat = 1\nmap bad() -> bool = C",
            "E_MIDDLE_RETURN_TYPE_MISMATCH",
        ),
        (
            "const C: bool = true\nmap bad() -> nat = C",
            "E_MIDDLE_RETURN_TYPE_MISMATCH",
        ),
        (
            "const C: nat = 1\nmap bad(s: Bit, b: bool) -> nat = select { s => C, default => b }",
            "E_MIDDLE_TYPE_MISMATCH",
        ),
    ] {
        let output = diagnostics(source);
        assert!(
            output.iter().any(|d| d.code.as_deref() == Some(code)),
            "{source}\n{output:?}"
        );
    }
    for other in ["UInt<16>", "Bit", "SInt<8>", "Bits<8>"] {
        for body in [
            "match s { 0 => N, 1 => x, _ => y }",
            "select priority { s => x, s => y, default => N }",
            "match s { 0 => select { s => N, default => x }, _ => y }",
        ] {
            let source =
                format!("map bad<N: nat>(s: Bit, x: UInt<8>, y: {other}) -> UInt<8> = {body}");
            let output = diagnostics(&source);
            assert!(
                output
                    .iter()
                    .any(|d| d.code.as_deref() == Some("E_MIDDLE_TYPE_MISMATCH")),
                "{source}\n{output:?}"
            );
        }
    }
}

#[test]
fn constant_nats_do_not_relax_software_function_type_checks() {
    for ty in ["Bit", "UInt<8>", "SInt<8>", "Bits<8>"] {
        for value in ["C", "C + 1"] {
            let source = format!("const C: nat = 1\nfn bad() -> {ty} {{ {value} }}");
            let output = diagnostics(&source);
            assert!(
                output
                    .iter()
                    .any(|d| d.code.as_deref() == Some("E_MIDDLE_RETURN_TYPE_MISMATCH")),
                "{source}\n{output:?}"
            );
            for body in [
                format!("let y: {ty} = {value}\n0"),
                format!("let y = match s {{ true => {value}, false => x }}\n0"),
                format!("let y = match s {{ true => x, false => {value} }}\n0"),
            ] {
                let source =
                    format!("const C: nat = 1\nfn bad(s: bool, x: {ty}) -> nat {{ {body} }}");
                let output = diagnostics(&source);
                assert!(
                    output
                        .iter()
                        .any(|d| d.code.as_deref() == Some("E_MIDDLE_TYPE_MISMATCH")),
                    "{source}\n{output:?}"
                );
            }
        }
    }
}
