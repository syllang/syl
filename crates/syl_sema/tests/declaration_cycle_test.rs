use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_syntax::SourceParser;

fn diagnostics(source: &str) -> Vec<syl_span::Diagnostic> {
    let ast = SourceParser::new(source).parse_file().expect(source);
    SemanticSession::new(&[ast]).check().diagnostics().to_vec()
}

#[test]
fn rejects_recursive_structs_after_generic_substitution() {
    for source in [
        "struct Rec { child: Rec }",
        "struct A { b: B }\nstruct B { a: A }",
        "struct Box<T> { value: T }\nstruct Rec { child: Box<Rec> }",
        "struct Box<T> { value: T }\nstruct Evil<T> { child: Evil<Box<T>> }",
        "struct Rec { children: [2]Rec }",
        "struct Box<T> { value: T }\nstruct Nest<T> { value: Box<T> }\nstruct Rec { child: Nest<Rec> }",
    ] {
        let diagnostics = diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_RECURSIVE_STRUCT")),
            "{source}\n{diagnostics:?}"
        );
    }
}

#[test]
fn accepts_finite_struct_instantiations_and_checks_field_types() {
    assert!(
        diagnostics("struct Box<T> { value: T }\nstruct Good { child: Box<Box<nat>> }").is_empty()
    );
    assert!(diagnostics("struct Empty<T> {}\nstruct Good { unused: Empty<Good> }").is_empty());
    let diagnostics = diagnostics("struct Bad { value: Missing }");
    assert_eq!(
        diagnostics[0].code.as_deref(),
        Some("E_MIDDLE_UNKNOWN_TYPE")
    );
}

#[test]
fn rejects_bundle_recursion_including_mixed_value_types() {
    for source in [
        "bundle Rec { child: Rec }",
        "bundle A { b: B }\nbundle B { a: A }",
        "bundle Box<T> { value: T }\nbundle Rec { child: Box<Rec> }",
        "bundle A { b: B }\nstruct B { a: A }",
    ] {
        let diagnostics = diagnostics(source);
        assert!(
            diagnostics
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_RECURSIVE_BUNDLE")),
            "{source}\n{diagnostics:?}"
        );
    }
}

#[test]
fn const_cycles_in_widths_conditions_ranges_and_struct_values_are_structured() {
    for source in [
        "const A: nat = B\nconst B: nat = A\ncell Top(y: out UInt<A>) { y := 0 }",
        "const A = B\nconst B = A",
        "const A = B + 1\nconst B = A + 1",
        "const A: bool = B\nconst B: bool = A\ncell Top() { if A {} }",
        "const A: nat = B\nconst B: nat = A\ncell Top() { for i in 0..A {} }",
        "struct P { x: nat }\nconst A: P = B\nconst B: P = A\ncell Top() { if A.x == 0 {} }",
    ] {
        let first = diagnostics(source);
        assert!(
            first
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_CONST_CYCLE")),
            "{source}\n{first:?}"
        );
        assert_eq!(first, diagnostics(source));
    }
}

#[test]
fn accepts_shared_acyclic_const_dependencies() {
    assert!(diagnostics("const A: nat = 1\nconst B: nat = A + A\nconst C: nat = B + A\ncell Top(y: out UInt<C>) { y := 0 }").is_empty());
}

#[test]
fn struct_field_const_cycles_are_bounded_in_widths_and_evaluation() {
    for source in [
        "struct P { x: nat }\nconst A: P = P { x: A.x }\nconst W: nat = A.x\nmap pass(x: UInt<W>) -> UInt<8> = x",
        "struct P { x: nat }\nconst A: P = P { x: A.x }\ncell Top() { if A.x == 0 {} }",
        "struct P { x: bool }\nconst A: P = P { x: A.x }\ncell Top() { if A.x {} }",
        "struct P { x: nat }\nconst A: P = P { x: B.x }\nconst B: P = P { x: A.x }\ncell Top() { for i in 0..A.x {} }",
        "struct P { x: nat }\nstruct Outer { p: P }\nconst A: Outer = Outer { p: P { x: A.p.x } }\nconst W: nat = A.p.x\nmap pass(x: UInt<W>) -> UInt<8> = x",
    ] {
        let first = diagnostics(source);
        assert!(
            first
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_CONST_CYCLE")),
            "{source}\n{first:?}"
        );
        assert_eq!(first, diagnostics(source));
    }
}

#[test]
fn shared_and_nested_acyclic_const_fields_keep_their_values() {
    for source in [
        "struct P { x: nat }\nconst A: P = P { x: 4 }\nconst W: nat = A.x + A.x\nmap pass(x: UInt<W>) -> UInt<8> = x",
        "struct P { x: nat }\nstruct Outer { p: P }\nconst A: Outer = Outer { p: P { x: 8 } }\nconst W: nat = A.p.x\nmap pass(x: UInt<W>) -> UInt<8> = x",
        "struct P { x: bool }\nconst A: P = P { x: true }\ncell Top() { if A.x && A.x {} }",
    ] {
        assert!(diagnostics(source).is_empty(), "{source}");
    }
    let mismatch = diagnostics(
        "struct P { x: nat }\nconst A: P = P { x: 4 }\nconst W: nat = A.x\nmap pass(x: UInt<W>) -> UInt<8> = x",
    );
    assert!(
        mismatch
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_RETURN_TYPE_MISMATCH")),
        "{mismatch:?}"
    );
}

#[test]
fn projected_const_fields_preserve_cross_module_owners_and_cycle_guards() {
    for (field, expected_code) in [
        ("8", None),
        ("4", Some("E_MIDDLE_RETURN_TYPE_MISMATCH")),
        ("A.p.x", Some("E_MIDDLE_CONST_CYCLE")),
    ] {
        let library = SourceParser::new(&format!(
            "struct P {{ x: nat }}\nstruct Outer {{ p: P }}\nconst A: Outer = Outer {{ p: P {{ x: {field} }} }}"
        ))
        .parse_file()
        .unwrap();
        let consumer = SourceParser::new(
            "use lib.A\nconst W: nat = A.p.x\nmap pass(x: UInt<W>) -> UInt<8> = x",
        )
        .parse_file()
        .unwrap();
        let session = SemanticSession::new_sources(vec![
            SemanticSourceFile::new(vec!["lib".into()], &library),
            SemanticSourceFile::new(vec!["app".into()], &consumer),
        ]);
        let first = session.check().diagnostics().to_vec();
        match expected_code {
            Some(code) => assert!(
                first.iter().any(|d| d.code.as_deref() == Some(code)),
                "{field}: {first:?}"
            ),
            None => assert!(first.is_empty(), "{field}: {first:?}"),
        }
        assert_eq!(first, session.check().diagnostics());
    }
}
