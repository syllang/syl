use syl_sema::SemanticSession;
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
