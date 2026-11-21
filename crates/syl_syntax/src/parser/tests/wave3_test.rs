use crate::{Item, SourceParser};

#[test]
fn parses_wave3_contextual_memory_reset_and_callable_attributes() {
    let source = "@cdc_sync(a, b, clk) cell Bridge<D: Domain>(clk: in Clock<D>, rst: in Reset<D, Async, ActiveLow>, a: in Bit) -> b: Bit { mem storage: [16] UInt<8>; reg sync: Bit reset(rst, 0, async, active_low); next sync := a; b := sync; }";
    let file = SourceParser::new(source).parse_file().unwrap();
    let Item::Cell(item) = &file.items[0] else {
        panic!("expected cell")
    };
    assert_eq!(item.attrs.len(), 1);
    assert_eq!(item.span.start, 0);
    assert!(matches!(item.body.stmts[0], crate::Stmt::Mem { .. }));
    let crate::Stmt::Reg {
        name,
        reset: Some(reset),
        ..
    } = &item.body.stmts[1]
    else {
        panic!("expected register")
    };
    assert_eq!(name, "sync");
    assert_eq!(reset.mode, Some(crate::ResetMode::Async));
    assert_eq!(reset.polarity, Some(crate::ResetPolarity::ActiveLow));
    let file = SourceParser::new(
        "@cdc_sync(a, b, clk) extern cell Bridge(clk: in Clock, a: in Bit) -> b: Bit",
    )
    .parse_file()
    .unwrap();
    let Item::ExternCell(item) = &file.items[0] else {
        panic!("expected extern cell")
    };
    assert_eq!(item.attrs.len(), 1);
    assert_eq!(item.span.start, 0);
}

#[test]
fn rejects_memory_initializers_and_duplicate_reset_qualifiers() {
    for source in [
        "cell Bad() { mem storage: [16] Bit = 0; }",
        "cell Bad() { mem storage: [16] Bit := 0; }",
        "cell Bad() { reg q: Bit reset(rst, 0, sync, async); }",
        "cell Bad() { reg q: Bit reset(rst, 0, active_high, active_low); }",
        "cell Bad() { reg q: Bit reset(rst, 0, unknown); }",
    ] {
        assert!(SourceParser::new(source).parse_file().is_err(), "{source}");
    }
    assert!(
        SourceParser::new("cell Fine(mem: in Bit) -> q: Bit { q := mem; }")
            .parse_file()
            .is_ok()
    );
}
