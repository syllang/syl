use super::*;
use crate::{SemanticTokenKind as Kind, SemanticTokenModifier as Modifier};

#[test]
fn workspace_symbols_rank_and_survive_broken_neighbors() {
    let text = "cell ReadyValidSlice() {} cell ready_valid() {} cell Ready() {} cell AlreadyReady() {} enum Mode { A, B } interface Bus { data: Bit, view Tx { out data } } struct Data { value: nat }";
    let (s, _) = workspace(&[
        ("good", text),
        ("broken", "cell Invalid(???"),
        ("badtype", "cell Bad(x: in Missing) {}"),
    ]);
    let names: Vec<_> = s
        .workspace_symbols("ready")
        .into_iter()
        .map(|s| s.name)
        .collect();
    assert_eq!(
        names,
        ["Ready", "ready_valid", "ReadyValidSlice", "AlreadyReady"]
    );
    assert_eq!(s.workspace_symbols("rvsl")[0].name, "ReadyValidSlice");
    let symbols = s.workspace_symbols("");
    assert!(symbols.iter().any(|s| s.name == "Data"));
    let variant = symbols.iter().find(|s| s.name == "A").unwrap();
    assert_eq!(variant.container_name.as_deref(), Some("good.Mode"));
    assert_eq!(source_at(&s, &variant.location), "A");
    assert!(
        symbols
            .iter()
            .any(|s| s.name == "Tx" && s.container_name.as_deref() == Some("good.Bus"))
    );
}

#[test]
fn hints_cover_types_widths_domains_and_range_filtering() {
    let text = "const W = 8; bundle Pair { a: Bit, b: UInt<3> } enum Mode { A, B, C } cell C<CoreClk: Domain>(clk: in Clock<CoreClk>, rst: in Reset<CoreClk>, data: in Pair, mode: in Mode, word: in UInt<W>, plain: in UInt<8>, bit: in Bit) { signal s := word; reg r: UInt<W> reset(clk, 0); } fn f() -> nat { let a = 1; a }";
    let (s, uri) = snapshot(text);
    assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
    let all = s.inlay_hints(
        &uri,
        SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(20, 0)),
    );
    for label in [": nat", ": UInt<W>", "[4b]", "[2b]", "[8b]", "@CoreClk"] {
        assert!(
            all.iter().any(|h| h.label == label),
            "missing {label}: {all:?}"
        );
    }
    assert!(
        all.iter()
            .any(|h| h.tooltip.as_deref() == Some("clock domain: CoreClk"))
    );
    assert!(
        all.iter()
            .any(|h| h.tooltip.as_deref() == Some("reset domain: CoreClk"))
    );
    let reg_type_end = text.find(" reset(clk").unwrap();
    assert!(
        all.iter()
            .any(|h| h.label == "@CoreClk" && h.position.character == reg_type_end),
        "register domain missing: {all:?}"
    );
    let pos = all.iter().find(|h| h.label == ": nat").unwrap().position;
    let restricted = s.inlay_hints(&uri, SourceRange::new(pos, pos));
    assert_eq!(restricted.len(), 1);
    assert_eq!(restricted[0].tooltip.as_deref(), Some("inferred type: nat"));
    assert!(
        s.inlay_hints(
            &uri,
            SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(0, 0))
        )
        .is_empty()
    );
}

#[test]
fn reset_domain_hints_preserve_identity_with_mode_and_polarity_qualifiers() {
    for qualifiers in [
        "",
        ", Sync, ActiveHigh",
        ", Sync, ActiveLow",
        ", Async, ActiveHigh",
        ", Async, ActiveLow",
    ] {
        let text = format!(
            "cell C<CoreClk: Domain>(clk: in Clock<CoreClk>, rst: in Reset<CoreClk{qualifiers}>) {{ reg q: Bit reset(rst, 0); next q := 0; }}"
        );
        let (s, uri) = snapshot(&text);
        assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
        let hints = s.inlay_hints(
            &uri,
            SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(1, 0)),
        );
        assert!(
            hints
                .iter()
                .any(|h| h.label == "@CoreClk"
                    && h.tooltip.as_deref() == Some("reset domain: CoreClk")),
            "{hints:?}"
        );
    }
}

#[test]
fn semantic_classes_modifiers_utf16_and_multiline_strings() {
    let text = "/// docs\nuse util.Sub;\nconst S = \"💡\ntext\";\n@packed bundle Word { bits: UInt<8> }\nenum Mode { A, B }\ninterface Bus { data: Bit, view Tx { out data } }\nfn f(x: nat) -> nat { let a = 1; a + x }\ncell Top(x: in Bit, y: out Bit) { signal s := x; reg r: Bit; next r := s; y := r; } // end\n";
    let (s, uris) = workspace(&[
        (
            "util",
            "extern cell Sub(x: in Bit)
",
        ),
        ("app", text),
    ]);
    let uri = &uris[1];
    let tokens = s.semantic_tokens(uri);
    let at = |needle: &str| {
        tokens
            .iter()
            .find(|t| t.range.start == position(text, needle))
            .unwrap()
    };
    assert_eq!(at("use").kind, Kind::Keyword);
    assert_eq!(at("util").kind, Kind::Namespace);
    assert_eq!(at("Sub;").kind, Kind::Struct);
    assert_eq!(at("Word").kind, Kind::Type);
    assert_eq!(at("A, B").kind, Kind::EnumMember);
    assert_eq!(at("packed").kind, Kind::Decorator);
    assert!(at("S =").modifiers.contains(&Modifier::Readonly));
    assert!(at("S =").modifiers.contains(&Modifier::Declaration));
    assert!(at("/// docs").modifiers.contains(&Modifier::Documentation));
    assert!(at("UInt<8>").modifiers.contains(&Modifier::DefaultLibrary));
    assert!(at("y: out").modifiers.contains(&Modifier::Modification));
    assert!(at("x: in").modifiers.contains(&Modifier::Readonly));
    for token in &tokens {
        assert_eq!(token.range.start.line, token.range.end.line);
        assert!(token.range.start.character < token.range.end.character);
    }
    for pair in tokens.windows(2) {
        let a = pair[0].range.end;
        let b = pair[1].range.start;
        assert!((a.line, a.character) <= (b.line, b.character));
    }
    assert_eq!(
        at("\"💡").range.end.character - at("\"💡").range.start.character,
        3
    );
}

#[test]
fn every_editor_query_checks_pre_cancelled_tokens() {
    let (s, uri) = snapshot("cell C(x: in Bit) { signal s := x; }");
    let t = CancellationToken::new();
    t.cancel();
    let p = SourcePosition::new(0, 5);
    let r = SourceRange::new(p, p);
    assert!(s.prepare_rename_at_with_token(&uri, p, &t).is_err());
    assert!(s.rename_at_with_token(&uri, p, "D", &t).is_err());
    assert!(s.workspace_symbols_with_token("", &t).is_err());
    assert!(s.inlay_hints_with_token(&uri, r, &t).is_err());
    assert!(s.semantic_tokens_with_token(&uri, &t).is_err());
    assert!(s.signature_help_at_with_token(&uri, p, &t).is_err());
}

#[test]
fn aggregate_array_and_explicit_enum_widths_are_concrete() {
    let text = "bundle Word<W: nat> { bits: UInt<W> } struct Pair { a: Bit, b: Bit } enum Mode: 4 { A, B } cell C(a: in Word<5>, b: in [3] Bit, c: in Pair, d: in Mode) {}";
    let (s, uri) = snapshot(text);
    assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
    let hints = s.inlay_hints(
        &uri,
        SourceRange::new(SourcePosition::new(0, 0), SourcePosition::new(1, 0)),
    );
    for label in ["[5b]", "[3b]", "[2b]", "[4b]"] {
        assert!(
            hints.iter().any(|h| h.label == label),
            "missing {label}: {hints:?}"
        );
    }
}
