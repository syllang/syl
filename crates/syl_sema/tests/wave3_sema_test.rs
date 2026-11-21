use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SemanticSession, SummaryCdcCrossing, SummaryCdcKind,
    SummaryLatencyClass, declaration_contracts,
};
use syl_syntax::SourceParser;

fn check(source: &str) -> syl_sema::SemanticOutput {
    let ast = SourceParser::new(source).parse_file().expect(source);
    SemanticSession::new(&[ast]).check()
}

fn rejects(source: &str, code: &str) {
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
fn reset_type_qualifiers_accept_defaults_and_reject_duplicates() {
    for ty in [
        "Reset",
        "Reset<D>",
        "Reset<D, Sync, ActiveHigh>",
        "Reset<D, Async, ActiveLow>",
        "Reset<D, Async>",
        "Reset<D, ActiveLow>",
        "Reset<Async, ActiveLow>",
    ] {
        let source = format!(
            "cell Good<D: Domain>(clk: in Clock<D>, rst: in {ty}) {{ reg sync: Bit reset(rst, 0) next sync := 0 }}"
        );
        let out = check(&source);
        assert!(
            out.diagnostics().is_empty(),
            "{source}\n{:?}",
            out.diagnostics()
        );
    }
    for ty in [
        "Reset<D, Sync, Async>",
        "Reset<D, ActiveHigh, ActiveLow>",
        "Reset<D, Unknown>",
        "Reset<Async, D>",
        "Reset<D, Async, ActiveLow, Sync>",
        "Reset<Bit>",
    ] {
        rejects(
            &format!("cell Bad<D: Domain>(rst: in {ty}) {{}}"),
            "E_MIDDLE_INVALID_RESET_QUALIFIER",
        );
    }
}

#[test]
fn cdc_attributes_validate_ports_directions_clock_and_signature() {
    for kind in [
        "cell Bridge(clk: in Clock, a: in Bit) -> b: Bit { b := a }",
        "extern cell Bridge(clk: in Clock, a: in Bit) -> b: Bit",
    ] {
        let source = format!("@cdc_sync(a, b, clk) {kind}");
        let output = check(&source);
        assert!(
            output.diagnostics().is_empty(),
            "{:?}",
            output.diagnostics()
        );
        let summary = output.opaque_summaries().unwrap().get("Bridge").unwrap();
        assert_eq!(
            summary.cdc_crossings(),
            &[SummaryCdcCrossing::new("a", "b", "clk")]
        );
        let ast = SourceParser::new(&source).parse_file().unwrap();
        assert!(
            declaration_contracts(&[], &source, &ast)
                .iter()
                .all(|c| c.signature_source.contains("@cdc_sync(a, b, clk)"))
        );
    }
    for attr in [
        "@cdc_sync(a, b)",
        "@cdc_sync(a, b, clk, a)",
        "@cdc_sync(1, b, clk)",
        "@cdc_sync(b, a, clk)",
        "@cdc_sync(a, missing, clk)",
        "@cdc_sync(a, b, a)",
        "@unknown(a, b, clk)",
    ] {
        rejects(
            &format!("{attr} extern cell Bridge(clk: in Clock, a: in Bit) -> b: Bit"),
            "E_MIDDLE_INVALID_CDC_ATTR",
        );
    }
}

#[test]
fn multiple_cdc_channels_and_host_overlay_preserve_the_contract() {
    let output = check(
        "@cdc_sync(a, b, clk) @cdc_sync(c, d, clk) extern cell Bridge(clk: in Clock, a: in Bit, c: in Bit, b: out Bit, d: out Bit)",
    );
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
    let table = output.opaque_summaries().unwrap();
    assert_eq!(table.get("Bridge").unwrap().cdc_crossings().len(), 2);
    let mut overlay = syl_sema::OpaqueSummaryTable::new();
    let crossing = SummaryCdcCrossing::new("a", "b", "clk").with_kind(SummaryCdcKind::Handshake);
    overlay.register(
        OpaqueItemSummary::builder(OpaqueItemKind::ExternCell, "Bridge")
            .cdc_crossing(crossing.clone())
            .build(),
    );
    let merged = table.merged(&overlay);
    assert_eq!(merged.get("Bridge").unwrap().cdc_crossings(), &[crossing]);
    assert!(!merged.get("Bridge").unwrap().endpoints().is_empty());
    overlay.register(OpaqueItemSummary::builder(OpaqueItemKind::ExternCell, "Bridge").build());
    assert_eq!(overlay.get("Bridge").unwrap().cdc_crossings().len(), 1);
}

#[test]
fn memory_intrinsics_check_types_targets_and_statement_context() {
    let good = "cell Ram(clk: in Clock, we: in Bit, addr: in UInt<4>, data: in UInt<8>) -> y: UInt<8> { mem storage: [16] UInt<8>; mem_write(storage, clk, we, addr, data); mem_read_sync(storage, clk, we, addr, y); }";
    let output = check(good);
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
    assert_eq!(
        output
            .opaque_summaries()
            .unwrap()
            .get("Ram")
            .unwrap()
            .latency_class(),
        SummaryLatencyClass::Sequential
    );
    for (body, code) in [
        ("mem storage: Bit;", "E_MIDDLE_MEMORY_REQUIRES_ARRAY_TYPE"),
        (
            "signal storage: [16] Bit; mem_write(storage, clk, we, addr, data);",
            "E_MIDDLE_MEMORY_TARGET_NOT_MEM",
        ),
        (
            "mem storage: [16] UInt<8>; signal x: Bit := mem_write(storage, clk, we, addr, data);",
            "E_MIDDLE_MEMORY_OP_STATEMENT_ONLY",
        ),
        (
            "mem storage: [16] UInt<8>; storage := data;",
            "E_MIDDLE_DIRECT_DRIVE_TO_MEM",
        ),
        (
            "mem storage: [16] UInt<8>; next storage := data;",
            "E_MIDDLE_DIRECT_DRIVE_TO_MEM",
        ),
        (
            "mem storage: [16] UInt<8>; mem_read_sync(storage, clk, we, addr, data);",
            "E_MIDDLE_NOT_DRIVABLE",
        ),
        (
            "mem storage: [16] UInt<8>; reg q: UInt<8>; mem_read_sync(storage, clk, we, addr, q);",
            "E_MIDDLE_CONTINUOUS_DRIVE_TO_REG",
        ),
        (
            "mem storage: [16] UInt<8>; mem_write(storage, we, we, addr, data);",
            "E_MIDDLE_TYPE_MISMATCH",
        ),
        (
            "mem storage: [16] UInt<8>; mem_write(storage, clk, addr, addr, data);",
            "E_MIDDLE_TYPE_MISMATCH",
        ),
        (
            "mem storage: [16] UInt<8>; mem_write(storage, clk, we, addr);",
            "E_MIDDLE_TYPE_MISMATCH",
        ),
    ] {
        rejects(
            &format!(
                "cell Bad(clk: in Clock, we: in Bit, addr: in UInt<4>, data: in UInt<8>) {{ {body} }}"
            ),
            code,
        );
    }
    rejects(
        "fn bad() { mem storage: [16] Bit; }",
        "E_MIDDLE_HARDWARE_STMT_IN_FN",
    );
}

#[test]
fn cdc_attribute_accepts_flattened_view_clock_and_data_fields() {
    let output = check(
        "interface Pins<T> { clk: Clock a: T b: T view bridge { in clk in a out b } } @cdc_sync(p_a, p_b, p_clk) extern cell Bridge(p: in Pins<Bit>.bridge)",
    );
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
}
