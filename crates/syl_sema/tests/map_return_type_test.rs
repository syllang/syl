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
