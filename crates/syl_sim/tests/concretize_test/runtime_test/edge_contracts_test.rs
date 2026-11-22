use super::*;

#[test]
fn extern_inout_formal_maps_only_its_connected_slice() {
    struct Pad {
        formal_width: u32,
        connected: u32,
    }
    impl ExternModel for Pad {
        fn eval(&mut self, i: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>, String> {
            let p = &i["p"];
            assert_eq!(p.width(), self.formal_width);
            assert_eq!(p.slice(0, self.connected), Value::z(self.connected));
            assert_eq!(
                p.slice(self.connected, self.formal_width - self.connected),
                Value::z(self.formal_width - self.connected)
            );
            Ok(BTreeMap::from([(
                "p".into(),
                "1".repeat(self.formal_width as usize).parse().unwrap(),
            )]))
        }
    }
    for (actual, formal, low, width) in [("p", 65, 0, 1), ("p[1]", 70, 65, 65), ("p[1]", 1, 65, 65)]
    {
        let (bus, ty) = if actual == "p" {
            (1, "Bit")
        } else {
            (130, "[2] UInt<65>")
        };
        let d = design(
            "Top",
            &format!(
                "extern cell Pad(p:inout UInt<{formal}>) cell Top(p:inout {ty}) {{let black=place Pad(p:{actual})}}"
            ),
        );
        let mut s = Simulator::new(&d);
        let connected = formal.min(width);
        s.register_extern(
            d.externs[0].instance,
            Pad {
                formal_width: formal,
                connected,
            },
        )
        .unwrap();
        s.eval_comb().unwrap();
        let p = s.read_named("p").unwrap();
        for bit in 0..bus {
            assert_eq!(
                p.bit(bit),
                if (low..low + connected).contains(&bit) {
                    Logic::One
                } else {
                    Logic::Z
                }
            );
        }
        let id = d.instances[d.externs[0].instance.0].ports[0].signal;
        assert_eq!(s.read(id).width(), formal);
    }
}

#[test]
fn native_and_external_flops_sample_together_with_new_clock_levels() {
    struct Dff {
        clk: Logic,
        q: Value,
    }
    impl ExternModel for Dff {
        fn eval(&mut self, i: &BTreeMap<String, Value>) -> Result<BTreeMap<String, Value>, String> {
            let clk = i["clk"].bit(0);
            if self.clk == Logic::Zero && clk == Logic::One {
                self.q = i["d"].clone();
            }
            self.clk = clk;
            Ok(BTreeMap::from([("o".into(), self.q.clone())]))
        }
    }
    let d = design(
        "Top",
        "extern cell Vendor(clk:in Clock,d:in Bit,o:out Bit)
      cell Top(pin:in Bit,i:in Bit,y:out Bit,echo:out Bit,last:out Bit) {
        signal clk: Clock := pin
        reg q:Bit
        next q:=i
        reg edge_level:Bit
        next edge_level:=pin
        echo:=edge_level
        signal external_q:Bit
        let black=place Vendor(clk:clk,d:q,o:external_q)
        y:=external_q
        reg tail:Bit
        next tail:=external_q
        last:=tail
      }",
    );
    let mut s = Simulator::new(&d);
    s.register_extern(
        d.externs[0].instance,
        Dff {
            clk: Logic::X,
            q: Value::x(1),
        },
    )
    .unwrap();
    drive(&mut s, "pin", 0);
    let mut old = Value::x(1);
    let mut older = Value::x(1);
    for i in [0, 1, 0, 1] {
        drive(&mut s, "i", i);
        s.tick("pin").unwrap();
        expect(&s, "echo", 1);
        assert_eq!(s.read_named("y").unwrap(), &old);
        assert_eq!(s.read_named("last").unwrap(), &older);
        older = old;
        old = Value::from_u64(1, i);
    }
}

#[test]
fn asynchronous_reset_samples_data_on_assertion_and_clock_only() {
    let d = design(
        "Top",
        "cell Top(clk:in Clock,release_pin:in Bit,i:in Bit)->y:Bit {
        signal rst: Reset<Async,ActiveLow> := release_pin
        reg q:Bit reset(rst,i)
        next q:=0
        y:=q }",
    );
    let mut s = Simulator::new(&d);
    drive(&mut s, "release_pin", 1);
    drive(&mut s, "i", 0);
    s.eval_comb().unwrap();
    drive(&mut s, "release_pin", 0);
    s.eval_comb().unwrap();
    expect(&s, "y", 0);
    drive(&mut s, "i", 1);
    s.eval_comb().unwrap();
    expect(&s, "y", 0);
    s.tick("clk").unwrap();
    expect(&s, "y", 1);
    drive(&mut s, "release_pin", 1);
    s.eval_comb().unwrap();
    expect(&s, "y", 1);
    s.tick("clk").unwrap();
    expect(&s, "y", 0);
}

#[test]
fn signed_decimal_division_remainder_and_unsigned_context() {
    let d=design("Top","cell Top(i:in Bit,u:in UInt<8>,q:out UInt<8>,r:out UInt<8>,m:out UInt<8>,nested:out UInt<8>,large:out UInt<32>,wide:out UInt<70>){
      q:=select priority {i => (-3)/2,default=>0,}
      r:=select priority {i => (-7)%3,default=>0,}
      m:=(-3)/u
      nested:=((-3)/2)+u
      large:=select priority {i=>2147483648/2,default=>0,}
      wide:=select priority {i=>(-3)/2,default=>0,}
    }");
    let mut s = Simulator::new(&d);
    drive(&mut s, "i", 1);
    drive(&mut s, "u", 2);
    s.eval_comb().unwrap();
    expect(&s, "q", 255);
    expect(&s, "r", 255);
    expect(&s, "m", 254);
    expect(&s, "nested", 0);
    expect(&s, "large", 1 << 30);
    assert_eq!(s.read_named("wide").unwrap().to_binary(), "1".repeat(70));
}

#[test]
fn assertions_require_known_deasserted_reset_for_both_polarities() {
    for (polarity, active) in [("ActiveHigh", 1), ("ActiveLow", 0)] {
        let d = design(
            "Top",
            &format!(
                "cell Top(clk:in Clock,rst:in Reset<Sync,{polarity}>,i:in Bit)->y:Bit {{
            reg q:Bit reset(rst,0)
            next q:=i
            assert(i)
            y:=q }}"
            ),
        );
        let mut s = Simulator::new(&d);
        drive(&mut s, "i", 0);
        for v in [Value::x(1), Value::z(1), Value::from_u64(1, active)] {
            s.drive_named("rst", v).unwrap();
            s.tick("clk").unwrap();
        }
        drive(&mut s, "rst", 1 - active);
        assert!(matches!(s.tick("clk"), Err(RuntimeError::Assertion(_))));
    }
}

#[test]
fn two_state_projects_after_all_inout_drivers_resolve() {
    let d = design("Top", "cell Top(p:inout Bit,i:in Bit) {p:=i}");
    let mut s = Simulator::with_mode(&d, StateMode::TwoState);
    drive(&mut s, "i", 1);
    s.eval_comb().unwrap();
    expect(&s, "p", 1);
    drive(&mut s, "p", 0);
    s.eval_comb().unwrap();
    expect(&s, "p", 0);
    s.drive_named("p", Value::z(1)).unwrap();
    s.eval_comb().unwrap();
    expect(&s, "p", 1);
    let d = design("Top", "cell Top(p:inout Bit) {p:=z()}");
    let mut s = Simulator::with_mode(&d, StateMode::TwoState);
    drive(&mut s, "p", 1);
    s.eval_comb().unwrap();
    expect(&s, "p", 1);
}

#[test]
fn construction_design_has_only_shared_public_access() {
    let d = design("Top", "cell Top(i:in Bit)->y:Bit {y:=i}");
    let s = Simulator::new(&d);
    assert!(std::ptr::eq(s.design(), &d));
    let surface = include_str!("../../../../../api/public-surface.txt");
    assert!(
        !surface
            .lines()
            .any(|line| line.starts_with("syl_sim|field|syl_sim::runtime::Simulator::design|"))
    );
}
