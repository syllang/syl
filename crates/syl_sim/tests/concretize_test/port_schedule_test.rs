use super::*;

fn lower_pick(width: u32, actual: &str, bit: u32) -> Result<SimDesign, SimError> {
    let source = format!(
        "cell Pick(i: in UInt<{width}>) -> o: Bit {{ o := i[{bit}] }}
         cell Top(i: in Bit, mask: in UInt<2>) -> o: Bit {{
             signal a: UInt<2>
             let p = place Pick(i: {actual})
             a[0] := i
             a[1] := p
             o := a[1]
         }}"
    );
    SimDesign::lower(&compile(&[("slice", &source)]), "Top", &BTreeMap::new())
}

fn position(design: &SimDesign, path: &str, bit: Option<u32>) -> usize {
    let selectors: Vec<_> = bit
        .map(|bit| Selector::Slice {
            low: bit,
            high: bit,
        })
        .into_iter()
        .collect();
    design
        .comb_eval_order
        .iter()
        .position(|id| {
            let target = &design.comb[id.0].target;
            design.signal(target.signal).source.path == path && target.selectors == selectors
        })
        .unwrap_or_else(|| panic!("missing write to {path} at {bit:?}"))
}

fn check_bit_read(design: &SimDesign, expr: &SimExpr, path: &str, bit: u32) {
    let SimExpr::Read(t) = expr else {
        panic!("expected a read of {path}[{bit}], found {expr:?}")
    };
    assert_eq!(design.signal(t.signal).source.path, path);
    assert_eq!(
        t.selectors,
        [Selector::Slice {
            low: bit,
            high: bit
        }]
    );
}

#[test]
fn port_slices_schedule_low_bits_before_feedback_and_report_real_cycles() {
    for (width, actual, op) in [
        (2, "a", None),
        (3, "a", None),
        (2, "a and mask", Some(HwBinaryOp::BitAnd)),
        (2, "a or mask", Some(HwBinaryOp::BitOr)),
        (2, "a xor mask", Some(HwBinaryOp::BitXor)),
    ] {
        let d = lower_pick(width, actual, 0).unwrap();
        let input_low = position(&d, "Top.p.i", Some(0));
        let input_high = position(&d, "Top.p.i", Some(1));
        let low = position(&d, "Top.a", Some(0));
        let child_output = position(&d, "Top.p.o", None);
        let child_result = position(&d, "Top.p", None);
        let high = position(&d, "Top.a", Some(1));
        assert!(low < input_low && input_low < child_output && child_output < child_result);
        assert!(child_result < high && high < position(&d, "Top.o", None) && high < input_high);
        for (step, bit) in [(input_low, 0), (input_high, 1)] {
            let value = &d.comb[d.comb_eval_order[step].0].value;
            if let Some(expected) = op {
                let SimExpr::Binary { op, left, right } = value else {
                    panic!("expected {expected:?}")
                };
                assert_eq!(*op, expected);
                check_bit_read(&d, left, "Top.a", bit);
                check_bit_read(&d, right, "Top.mask", bit);
            } else {
                check_bit_read(&d, value, "Top.a", bit);
            }
        }
        if width == 3 {
            let padding = position(&d, "Top.p.i", Some(2));
            assert_eq!(d.comb[d.comb_eval_order[padding].0].value, SimExpr::Zero);
        }

        let Err(SimError::CombinationalCycle { chain, .. }) = lower_pick(width, actual, 1) else {
            panic!("expected feedback through {actual}")
        };
        assert_eq!(chain.first(), chain.last());
        for path in ["Top.a", "Top.p.i", "Top.p.o", "Top.p"] {
            assert!(chain.iter().any(|p| p == path));
        }
    }
}

#[test]
fn truncating_port_reads_only_the_low_bits_even_without_an_interior_cut() {
    let d = lower_pick(1, "a", 0).unwrap();
    let input = position(&d, "Top.p.i", None);
    check_bit_read(&d, &d.comb[d.comb_eval_order[input].0].value, "Top.a", 0);
    assert!(position(&d, "Top.a", Some(0)) < input);
    assert!(input < position(&d, "Top.p.o", None));
    assert!(position(&d, "Top.p", None) < position(&d, "Top.a", Some(1)));
}
