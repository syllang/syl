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
