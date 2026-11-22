use super::*;

#[test]
fn lane_array_transfers_independent_packed_words() {
    let d = design("LaneArray", LANES);
    let mut s = Simulator::new(&d);
    drive(&mut s, "in_streams_valid", 0);
    drive(&mut s, "out_streams_ready", 0);
    reset(&mut s);
    drive(&mut s, "lane_ids", 0x3c2b1a09);
    let inputs: Vec<_> = (0..4u64)
        .rev()
        .map(|i| Value::from_u64(41, ((100 * i + 7) << 9) | (i % 2)))
        .collect();
    s.drive_named("in_streams_payload", Value::concat(&inputs).unwrap())
        .unwrap();
    drive(&mut s, "in_streams_valid", 15);
    drive(&mut s, "out_streams_ready", 5);
    tick(&mut s);
    drive(&mut s, "in_streams_valid", 0);
    drive(&mut s, "in_streams_payload", 0);
    s.eval_comb().unwrap();
    expect(&s, "out_streams_valid", 10);
    expect(&s, "in_streams_ready", 5);
    let payload = s.read_named("out_streams_payload").unwrap();
    for i in [1, 3] {
        let tag = (0x3c2b1a09 >> (8 * i)) & 255;
        assert_eq!(
            payload.slice(i * 41, 41).as_u64(),
            Some(u64::from(((100 * i + 8) << 9) | (tag << 1) | 1))
        );
    }
    drive(&mut s, "out_streams_ready", 15);
    tick(&mut s);
    expect(&s, "out_streams_valid", 0);
}
#[test]
fn slice_schedules_execute_through_extended_and_truncated_ports() {
    for (width, expr) in [
        (1, "a"),
        (3, "a"),
        (2, "a and mask"),
        (2, "a or mask"),
        (2, "a xor mask"),
    ] {
        let src = format!(
            "cell Pick(i: in UInt<{width}>) -> o: Bit {{ o := i[0] }}
             cell Top(i: in Bit, mask: in UInt<2>) -> o: Bit {{
             signal a: UInt<2>
             let p = place Pick(i: {expr})
             a[0] := i
             a[1] := p
             o := a[1]
         }}"
        );
        let d = design("Top", &src);
        let mut s = Simulator::new(&d);
        for (i, m) in [(1, 1), (0, 3), (1, 0)] {
            drive(&mut s, "i", i);
            drive(&mut s, "mask", m);
            s.eval_comb().unwrap();
            expect(
                &s,
                "o",
                match expr {
                    "a and mask" => i & m,
                    "a or mask" => (i | m) & 1,
                    "a xor mask" => (i ^ m) & 1,
                    _ => i,
                },
            );
        }
    }
}
#[test]
fn source_located_assertion_samples_old_state_and_masks_reset() {
    let src = "cell Guard<D: Domain>(clk: in Clock<D>, rst: in Reset<D>, i: in Bit, y: out Bit) {
        reg q: Bit reset(rst,0)
        next q := i
        assert(q)
        y := q
    }
    cell Top<D: Domain>(clk: in Clock<D>, rst: in Reset<D>, i: in Bit) -> y: Bit {
        let guarded = place Guard<D>(clk:clk,rst:rst,i:i,y:y)
    }";
    let d = design("Top", src);
    let mut s = Simulator::new(&d);
    reset(&mut s);
    drive(&mut s, "i", 1);
    let Err(RuntimeError::Assertion(f)) = s.tick("clk") else {
        panic!("expected assertion")
    };
    assert_eq!(f.message, "assert failed");
    assert_eq!(f.cycle, 3);
    assert!(f.path.starts_with("Top.guarded."));
    assert_eq!(&src[f.span.start..f.span.end], "assert(q)");
    expect(&s, "y", 0);
}
#[test]
fn overlap_records_source_and_preserves_mux_priority() {
    let src =
        "cell Top(a: in Bit,b: in Bit)-> y: UInt<2> { y := select unique {a=>1,b=>2,default=>0,} }";
    let d = design("Top", src);
    let mut s = Simulator::new(&d);
    drive(&mut s, "a", 1);
    drive(&mut s, "b", 1);
    s.eval_comb().unwrap();
    expect(&s, "y", 1);
    let v = &s.unique_violations[0];
    assert_eq!(v.path, "Top.y");
    assert!(src[v.span.start..v.span.end].contains("select unique"));
    s.strict_unique = true;
    assert!(matches!(s.eval_comb(), Err(RuntimeError::UniqueOverlap(_))));
    s.drive_named("a", Value::x(1)).unwrap();
    s.eval_comb().unwrap();
    assert_eq!(s.read_named("y").unwrap().to_binary(), "xx");
}
#[test]
fn stateful_extern_callback_scheduled_between_producer_and_consumer() {
    let d = design(
        "Top",
        "extern cell Vendor(i: in Bit, o: out Bit)
    cell Top(i: in Bit)-> y: Bit {signal mid: Bit
    let black = place Vendor(i: not i, o:mid)
    y := not mid }",
    );
    let mut s = Simulator::new(&d);
    drive(&mut s, "i", 0);
    assert!(s.eval_comb().unwrap_err().to_string().contains("Top.black"));
    struct Model(usize);
    impl ExternModel for Model {
        fn eval(
            &mut self,
            input: &BTreeMap<String, Value>,
        ) -> Result<BTreeMap<String, Value>, String> {
            self.0 += 1;
            assert!(self.0 <= 2);
            Ok(BTreeMap::from([("o".into(), input["i"].clone())]))
        }
    }
    s.register_extern(d.externs[0].instance, Model(0)).unwrap();
    for n in [0, 0, 1, 1] {
        drive(&mut s, "i", n);
        s.eval_comb().unwrap();
        expect(&s, "y", n);
    }
}
#[test]
fn vcd_filters_scopes_and_compresses_four_state_changes() {
    let d = design("LaneArray", LANES);
    let mut s = Simulator::new(&d);
    let port = s.port("in_streams_payload").unwrap();
    let mut w = VcdWriter::filtered(Vec::new(), &d, |id, path| {
        id == port || path.starts_with("LaneArray.lanes[0].")
    })
    .unwrap();
    s.eval_comb().unwrap();
    w.sample(0, &s).unwrap();
    s.drive(port, Value::z(164)).unwrap();
    s.eval_comb().unwrap();
    w.sample(5, &s).unwrap();
    w.sample(9, &s).unwrap();
    let text = String::from_utf8(w.finish().unwrap()).unwrap();
    for name in ["LaneArray", "lanes[0]", "buffered"] {
        assert!(text.contains(&format!("$scope module {name} $end")))
    }
    assert!(!text.contains("$scope module lanes[1]"));
    assert!(text.contains(&format!("b{} s{}", "z".repeat(164), port.0)));
    assert!(text.contains(&format!("b{} s{}", "x".repeat(164), port.0)));
    assert!(text.ends_with("#9\n"));
    assert_eq!(
        text.matches("$scope module").count(),
        text.matches("$upscope").count()
    );
}

#[test]
fn dynamic_out_of_bounds_and_two_state_projection() {
    // Exercise the concrete IR independently of static bounds diagnostics.
    let mut d = design(
        "Top",
        "cell Top(a: in UInt<4>,i: in UInt<3>)-> y: Bit { y := a[0] }",
    );
    let index = d.top_ports()[1].signal;
    let SimExpr::Read(t) = &mut d.comb[0].value else {
        panic!("expected projection")
    };
    t.selectors = vec![Selector::Index {
        index: Box::new(SimExpr::Read(SimTarget {
            signal: index,
            selectors: vec![],
        })),
        width: 1,
    }];
    for (mode, unknown) in [
        (StateMode::FourState, Value::x(1)),
        (StateMode::TwoState, Value::zero(1)),
    ] {
        let mut s = Simulator::with_mode(&d, mode);
        drive(&mut s, "a", 8);
        for i in [0, 3, 4, 7] {
            drive(&mut s, "i", i);
            s.eval_comb().unwrap();
            if i < 4 {
                expect(&s, "y", (i == 3).into())
            } else {
                assert_eq!(s.read_named("y").unwrap(), &unknown)
            }
        }
    }
}
#[test]
fn shared_inout_resolves_conflict_and_released_drivers() {
    let d = design(
        "Top",
        "cell Pad(p: inout Bit, i: in Bit) {p := i}
        cell Top(p: inout Bit,i: in Bit) { let pad=place Pad(p:p,i:i) }",
    );
    let mut s = Simulator::new(&d);
    drive(&mut s, "i", 1);
    s.eval_comb().unwrap();
    expect(&s, "p", 1);
    drive(&mut s, "p", 0);
    s.eval_comb().unwrap();
    assert_eq!(s.read_named("p").unwrap(), &Value::x(1));
    s.drive_named("i", Value::z(1)).unwrap();
    s.eval_comb().unwrap();
    expect(&s, "p", 0);
    s.drive_named("p", Value::z(1)).unwrap();
    s.eval_comb().unwrap();
    assert_eq!(s.read_named("p").unwrap(), &Value::z(1));
}

#[test]
fn top_pin_and_its_local_clock_copy_trigger_one_group() {
    let d = design(
        "Top",
        "cell Top(pin:in Bit,i:in Bit)->o:Bit {
        signal clk: Clock := pin
        reg q: Bit
        next q:=i
        o:=q
    }",
    );
    let mut s = Simulator::new(&d);
    drive(&mut s, "i", 1);
    s.tick("pin").unwrap();
    expect(&s, "o", 1);
}
