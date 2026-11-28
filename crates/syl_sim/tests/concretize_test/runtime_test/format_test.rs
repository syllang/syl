use super::*;

#[test]
fn rising_edge_formats_old_state_and_reports_the_same_fault_cycle() {
    let d = design(
        "T",
        r#"
        enum State { Idle, Busy, Done }
        bundle Packet { valid: Bit, addr: UInt<8> }
        struct Config { enabled: bool, limit: nat }
        cell T(clk: in Clock, rst: in Reset, en: in Bit, data: in UInt<8>, s: in SInt<8>, state: in State, pkt: in Packet) {
            var cfg = Config { enabled: true, limit: 3 }
            reg old: UInt<8> reset(rst, 0)
            next old := data
            log(when: en, "{{{data:#X}}} {old:d} {data + data:u} {pkt.addr:x} {data[7:4]} {1:b} {:#04x} {0:s} {s} {data:#b} {en:?} {data:?} {state}/{state:?} {pkt} {pkt:#x} {cfg}", data, en)
            assert(not en, "fault {state} {old:#x}")
        }"#,
    );
    let mut sim = Simulator::new(&d);
    drive(&mut sim, "data", 42);
    drive(&mut sim, "s", 255);
    drive(&mut sim, "state", 1);
    drive(&mut sim, "pkt", 298);
    drive(&mut sim, "en", 1);
    drive(&mut sim, "rst", 1);
    tick(&mut sim);
    drive(&mut sim, "rst", 0);
    drive(&mut sim, "en", 0);
    tick(&mut sim);
    assert!(sim.log_lines.is_empty());
    drive(&mut sim, "data", 250);
    drive(&mut sim, "en", 1);
    let Err(RuntimeError::Assertion(failure)) = sim.tick("clk") else {
        panic!("expected failure")
    };
    assert_eq!(failure.cycle, 3);
    assert_eq!(failure.message, "fault State.Busy 0x2a");
    assert_eq!(
        sim.log_lines,
        [
            "[cycle 3] T: {0XFA} 42 244 2a 15 1 0xfa -6 -1 0b11111010 1'b1 8'hfa State.Busy/State.Busy(2'd1) { valid: 1, addr: 42 } { valid: 0x1, addr: 0x2a } { enabled: true, limit: 3 }"
        ]
    );
}

#[test]
fn explicit_clock_and_active_low_reset_stream_four_state_values() {
    let d = design(
        "T",
        r#"
        enum State { Idle, Busy, Done }
        cell T<A:Domain,B:Domain>(a:in Clock<A>, b:in Clock<B>, rb:in Reset<B,Sync,ActiveLow>, en:in Bit, state:in State, data:in UInt<8>) {
            log(clk:b, rst:rb, when:en,"{state} {data:#b} {data:x}")
        }"#,
    );
    let mut lines = Vec::new();
    {
        let mut sim = Simulator::new(&d);
        sim.set_log_sink(|line| lines.push(line.to_owned()));
        drive(&mut sim, "rb", 0);
        drive(&mut sim, "en", 1);
        sim.tick("b").unwrap();
        drive(&mut sim, "rb", 1);
        sim.tick("a").unwrap();
        sim.drive_named("en", Value::x(1)).unwrap();
        sim.tick("b").unwrap();
        drive(&mut sim, "en", 1);
        sim.drive_named("state", Value::x(2)).unwrap();
        sim.drive_named("data", "xxxxzzzz".parse().unwrap())
            .unwrap();
        sim.tick("b").unwrap();
        drive(&mut sim, "state", 3);
        sim.tick("b").unwrap();
        assert!(sim.log_lines.is_empty());
    }
    assert_eq!(
        lines,
        [
            "[cycle 4] T: State(2'bxx) 0bxxxxzzzz xz",
            "[cycle 5] T: State(3) 0bxxxxzzzz xz"
        ]
    );
}

#[test]
fn formatting_nested_software_fields_preserves_generic_values() {
    let source = r#"struct Sub { n:nat }
        struct Config { flag:bool, sub:Sub }
        cell U<N:nat>(clk:in Clock) {
            var cfg = Config { flag:true, sub:Sub { n:N } }
            log("{cfg}")
        }"#;
    let design = SimDesign::lower(
        &compile(&[("fixture", source)]),
        "U",
        &BTreeMap::from([("N".into(), ParamValue::Nat(7))]),
    )
    .unwrap();
    let mut sim = Simulator::new(&design);
    tick(&mut sim);
    assert_eq!(
        sim.log_lines,
        ["[cycle 1] U: { flag: true, sub: { n: 7 } }"]
    );
}
