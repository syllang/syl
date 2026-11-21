use super::*;

fn help(text: &str) -> crate::SignatureHelpResult {
    let offset = text.find('|').unwrap();
    let cleaned = text.replacen('|', "", 1);
    let (s, uri) = snapshot(&cleaned);
    s.signature_help_at(&uri, SourcePosition::new(0, offset))
        .unwrap_or_else(|| panic!("no signature for {text}"))
}

#[test]
fn positional_named_nested_generic_and_incomplete_calls() {
    for (text, expected) in [
        ("fn f(a: nat, b: nat) -> nat { a } const X = f(1, |2);", 1),
        (
            "extern cell Sub(a: in Bit, b: in Bit)  cell Top(x: in Bit) { place Sub(b: x, a: |x); }",
            0,
        ),
        (
            "extern cell Sub(a: in Bit, b: in Bit)  cell Top(x: in Bit) { place Sub(a: x, |",
            1,
        ),
        (
            "map add<W: nat>(a: UInt<W>, b: UInt<W>) -> UInt<W> = a + b; cell Top(x: in UInt<8>) { signal y := add<8>(x, |x); }",
            1,
        ),
        (
            "fn f(a: nat, b: nat) -> nat { a } const X = f(f(1, 2), |3);",
            1,
        ),
    ] {
        let h = help(text);
        assert_eq!(h.active_parameter, Some(expected), "{text}: {h:?}");
        assert_eq!(h.signatures[0].parameters.len(), 2);
    }
}

#[test]
fn method_calls_strip_receiver_and_keep_documentation() {
    let text = "bundle Word { bits: UInt<8> } fn count(this word: Word, extra: nat) -> nat { extra } cell C(word: in Word) { const n = word.count(|1); }";
    let h = help(text);
    assert_eq!(h.signatures[0].parameters.len(), 1, "{h:?}");
    assert_eq!(h.signatures[0].parameters[0].label, "extra: nat");
    assert!(!h.signatures[0].label.contains("this"));
}

#[test]
fn non_call_positions_and_comments_do_not_offer_signatures() {
    let text = "fn f(a: nat) -> nat { a } const X = f(1); // f(";
    let (s, uri) = snapshot(text);
    assert!(
        s.signature_help_at(&uri, SourcePosition::new(0, text.len()))
            .is_none()
    );
    assert!(s.signature_help_at(&uri, position(text, "const")).is_none());
}

#[test]
fn signature_docs_empty_parameters_and_cell_results() {
    let text = "/// gate docs\ncell Gate<W: nat = 8,>(\n/// input docs\nx: in UInt<W>) -> result: UInt<W> { result := x }\ncell Top(x: in UInt<8>) { place Gate(x: x); }";
    let (s, uri) = snapshot(text);
    assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
    let help = s.signature_help_at(&uri, position(text, "x); }")).unwrap();
    let sig = &help.signatures[0];
    assert_eq!(sig.documentation.as_deref(), Some("gate docs"));
    assert_eq!(
        sig.parameters[0].documentation.as_deref(),
        Some("input docs")
    );
    assert!(sig.label.contains("<W: nat = 8,>"), "{sig:?}");
    assert!(sig.label.ends_with("-> result: UInt<W>"), "{sig:?}");
    let help = self::help("fn zero() -> nat { 0 } const X = zero(|);");
    assert_eq!(help.active_parameter, None);
    assert!(help.signatures[0].parameters.is_empty());
}

#[test]
fn comparisons_do_not_hide_argument_separators() {
    for (text, expected) in [
        (
            "fn f(a: bool, b: nat) -> nat { b } const X = f(1 < 2, |3);",
            1,
        ),
        ("fn f(a: bool, b: nat) -> nat { b } const X = f(1 < 2, |", 1),
        (
            "fn f(a: bool, b: bool, c: nat) -> nat { c } const X = f(1 < 2, 3 > 2, |4);",
            2,
        ),
        (
            "map g<A, B>(x: nat) -> bool = true fn f(a: bool, b: nat) -> nat { b } const X = f(g<UInt<8>, Bit>(1), |3);",
            1,
        ),
        (
            "fn g(a: bool, b: nat) -> nat { b } fn f(a: nat, b: nat, c: nat) -> nat { b } const X = f(g(3 > 2, 1), |3, 4);",
            1,
        ),
    ] {
        let result = help(text);
        assert_eq!(result.active_parameter, Some(expected), "{text}");
        assert_eq!(
            result.signatures[0].active_parameter,
            Some(expected),
            "{text}"
        );
    }
}

#[test]
fn declaration_parameters_are_not_call_sites() {
    for text in [
        "fn f(|a: bool, b: nat) -> nat { b } const X = f(1 < 2, 3);",
        "fn f(a: bool, |b: nat) -> nat { b }",
        "cell C(|x: in Bit) {}",
        "cell C<W: nat>(|x: in UInt<W>) {}",
        "map m(|x: Bit) -> Bit = x",
        "map m<W: nat>(|x: UInt<W>) -> UInt<W> = x",
        "extern cell C(|x: in Bit)",
        "extern cell C<W: nat>(|x: in UInt<W>)",
    ] {
        let offset = text.find('|').unwrap();
        let cleaned = text.replacen('|', "", 1);
        let (s, uri) = snapshot(&cleaned);
        assert!(s.diagnostics().is_empty(), "{text}: {:?}", s.diagnostics());
        assert!(
            s.signature_help_at(&uri, SourcePosition::new(0, offset))
                .is_none(),
            "declaration offered signature help: {text}"
        );
    }
}
