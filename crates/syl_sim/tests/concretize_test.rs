use std::collections::{BTreeMap, BTreeSet};
use syl_elab::{ElaborationOutput, HardwareCompiler};
use syl_sema::{SemanticSession, SemanticSourceFile};
use syl_sim::*;
use syl_span::SourceId;
use syl_syntax::SourceParser;

fn compile(files: &[(&str, &str)]) -> ElaborationOutput {
    let ast: Vec<_> = files
        .iter()
        .enumerate()
        .map(|(i, (_, s))| {
            SourceParser::new_in(s, SourceId::new(i))
                .parse_file()
                .unwrap()
        })
        .collect();
    let sources = files
        .iter()
        .zip(&ast)
        .map(|((path, _), ast)| {
            SemanticSourceFile::new(path.split('.').map(str::to_owned).collect(), ast)
        })
        .collect();
    let hir = SemanticSession::new_sources(sources).resolve_hir().unwrap();
    let tir = hir.check_tir().unwrap();
    let output = HardwareCompiler::new().output_for_tir(&tir);
    assert!(output.hwir().is_some(), "{:?}", output.diagnostics());
    output
}

const STREAM: &str = include_str!("../../../examples/std/stream.syl");
const LANES: &str = include_str!("../../../examples/mvp/lane_array.syl");
const RAM: &str = include_str!("../../../examples/std/mem.syl");
const CDC: &str = include_str!("../../../examples/std/cdc.syl");

#[test]
fn standard_library_concretizes_every_simulation_top() {
    let sources = [
        ("std.stream", STREAM),
        ("std.pulse", include_str!("../../../examples/std/pulse.syl")),
        ("std.skid", include_str!("../../../examples/std/skid.syl")),
        (
            "std.arbiter",
            include_str!("../../../examples/std/arbiter.syl"),
        ),
        ("std.cdc", CDC),
        ("std.mem", RAM),
        ("std.fifo", include_str!("../../../examples/std/fifo.syl")),
    ];
    let output = compile(&sources);
    for top in [
        "RisingEdgeDetector",
        "FallingEdgeDetector",
        "EdgeDetector",
        "PulseStretcher",
        "WordSkidBuffer",
        "FixedPriorityArbiter4",
        "RoundRobinArbiter4",
        "Sync2FF",
        "Sync2FFAsyncLow",
        "PulseSynchronizer",
        "SinglePortRam",
        "SimpleDualPortRam",
        "DistributedRam",
        "SyncFifo",
        "StreamSyncFifo",
    ] {
        let design = SimDesign::lower(&output, top, &BTreeMap::new())
            .unwrap_or_else(|e| panic!("{top}: {e}"));
        assert_eq!(
            design.comb_eval_order.iter().collect::<BTreeSet<_>>().len(),
            design.comb.len()
        );
        assert!(design.signals.iter().all(|s| s.width > 0));
        if top == "PulseSynchronizer" {
            assert_eq!(design.clocks.len(), 2);
            assert_eq!(design.regs.len(), 4);
        }
    }
}

#[test]
fn generated_lanes_share_templates_preserve_slices_and_paths() {
    let output = compile(&[("std.stream", STREAM), ("lane_array", LANES)]);
    for (params, n, w) in [
        (BTreeMap::new(), 4, 32),
        (
            BTreeMap::from([
                ("N".into(), ParamValue::Nat(3)),
                ("W".into(), ParamValue::Nat(17)),
            ]),
            3,
            17,
        ),
    ] {
        let design = SimDesign::lower(&output, "LaneArray", &params).unwrap();
        let (template, family) = design
            .families
            .iter()
            .enumerate()
            .find(|(_, f)| f.module.last().unwrap() == "Lane")
            .unwrap();
        assert_eq!(family.members.len(), n);
        for (i, id) in family.members.iter().enumerate() {
            let lane = &design.instances[id.0];
            assert_eq!(lane.template, TemplateId(template));
            assert_eq!(lane.source.path, format!("LaneArray.lanes[{i}]"));
            assert!(
                design
                    .instances
                    .iter()
                    .any(|s| s.source.path == format!("LaneArray.lanes[{i}].buffered"))
            );
            let span = lane.source.span();
            assert!(LANES[span.start..span.end].contains("place Lane"));
        }
        let bus = design
            .top_ports()
            .iter()
            .find(|p| p.name == "in_streams_payload")
            .unwrap();
        assert_eq!(design.signal(bus.signal).width, n as u32 * (w + 9));
        let mut slices = design.comb.iter().filter_map(|a| match &a.value {
            SimExpr::Read(t) if t.signal == bus.signal => Some(&t.selectors),
            _ => None,
        });
        for i in 0..n as u32 {
            assert_eq!(
                slices.next().unwrap().as_slice(),
                &[Selector::Slice {
                    low: i * (w + 9),
                    high: (i + 1) * (w + 9) - 1
                }]
            );
        }
        assert_eq!(design.clocks.len(), 1);
        assert_eq!(design.resets.len(), 1);
        assert_eq!(
            design.comb_eval_order,
            SimDesign::lower(&output, "LaneArray", &params)
                .unwrap()
                .comb_eval_order
        );
    }
    assert!(
        SimDesign::lower(
            &output,
            "LaneArray",
            &BTreeMap::from([("N".into(), ParamValue::Nat(0))])
        )
        .is_err()
    );
}

#[test]
fn sequential_domains_memory_ports_and_origins() {
    let output = compile(&[("cdc", CDC), ("mem", RAM)]);
    for (top, mode, polarity) in [
        ("Sync2FF", HwResetMode::Sync, HwResetPolarity::ActiveHigh),
        (
            "Sync2FFAsyncLow",
            HwResetMode::Async,
            HwResetPolarity::ActiveLow,
        ),
    ] {
        let d = SimDesign::lower(&output, top, &BTreeMap::new()).unwrap();
        let r = d.register(RegId(0));
        let domain = &d.resets[r.reset.as_ref().unwrap().0.0];
        assert_eq!((domain.mode, domain.polarity), (mode, polarity));
        let span = r.source.span();
        assert!(CDC[span.start..span.end].contains("reg stage1"));
        assert_eq!(d.signal(d.clocks[r.clock.0].signal).source.name, "dst_clk");
    }
    for (top, sync, comb) in [
        ("SinglePortRam", 1, 0),
        ("SimpleDualPortRam", 1, 0),
        ("DistributedRam", 0, 1),
    ] {
        let d = SimDesign::lower(
            &output,
            top,
            &BTreeMap::from([
                ("W".into(), ParamValue::Nat(37)),
                ("DEPTH".into(), ParamValue::Nat(13)),
            ]),
        )
        .unwrap();
        let mem = &d.memories[0];
        assert_eq!((mem.width, mem.depth), (37, 13));
        assert_eq!(
            (mem.writes.len(), mem.sync_reads.len(), mem.comb_reads.len()),
            (1, sync, comb)
        );
        let span = mem.source.span();
        assert_eq!(span.source, SourceId::new(1));
        assert!(RAM[span.start..span.end].contains("mem storage"));
        let port = |name: &str| {
            let signal = d
                .top_ports()
                .iter()
                .find(|p| p.name == name)
                .unwrap()
                .signal;
            SimTarget {
                signal,
                selectors: vec![],
            }
        };
        let read = |name| SimExpr::Read(port(name));
        let and = |left, right| SimExpr::Binary {
            op: HwBinaryOp::BitAnd,
            left: Box::new(left),
            right: Box::new(right),
        };
        let write = &mem.writes[0];
        let single = top == "SinglePortRam";
        assert_eq!(write.address, read(if single { "addr" } else { "waddr" }));
        assert_eq!(write.data, read("wdata"));
        assert_eq!(
            write.enable,
            if single {
                and(read("en"), read("we"))
            } else {
                read("we")
            }
        );
        let expected_output = if let Some(sync) = mem.sync_reads.first() {
            let not_write = SimExpr::Unary {
                op: HwUnaryOp::Not,
                expr: Box::new(read("we")),
            };
            assert_eq!(
                sync.enable,
                if single {
                    and(read("en"), not_write)
                } else {
                    read("re")
                }
            );
            assert_eq!(sync.address, read(if single { "addr" } else { "raddr" }));
            SimExpr::Read(sync.target.clone())
        } else {
            assert_eq!(mem.comb_reads[0].address, read("raddr"));
            SimExpr::MemoryRead {
                memory: MemId(0),
                port: 0,
            }
        };
        assert!(
            d.comb
                .iter()
                .any(|a| a.target == port("rdata") && a.value == expected_output)
        );
    }
}

const CROSS: &str = r#"
cell Relay(i: in Bit, o: out Bit) { o := i }
cell Cross(y: out Bit) {
    signal a: Bit
    signal b: Bit
    let left = place Relay(i: b, o: a)
    let right = place Relay(i: a, o: b)
    y := a
}
"#;

#[test]
fn tarjan_reports_a_closed_signal_chain_through_both_cells() {
    let output = compile(&[("cross", CROSS)]);
    SimDesign::lower(&output, "Relay", &BTreeMap::new()).unwrap();
    let err = SimDesign::lower(&output, "Cross", &BTreeMap::new()).unwrap_err();
    let SimError::CombinationalCycle { chain, members } = err else {
        panic!("{err}")
    };
    assert_eq!(chain.first(), chain.last());
    let expected = [
        "Cross.left.i",
        "Cross.left.o",
        "Cross.a",
        "Cross.right.i",
        "Cross.right.o",
        "Cross.b",
    ];
    for name in expected {
        assert!(members.iter().any(|n| n == name));
    }
    let start = chain.iter().position(|n| n == expected[0]).unwrap();
    for (i, path) in expected.iter().enumerate() {
        assert_eq!(&chain[(start + i) % expected.len()], path);
    }
}

#[test]
fn extern_boundaries_and_assertion_source_survive_lowering() {
    let source = r#"
extern cell Vendor(i: in Bit, o: out Bit)
cell Top<D: Domain>(clk: in Clock<D>, rst: in Reset<D, Async, ActiveLow>, i: in Bit, y: out Bit) {
    assert(i)
    let black = place Vendor(i: i, o: y)
}
"#;
    let output = compile(&[("top", source)]);
    let d = SimDesign::lower(&output, "Top", &BTreeMap::new()).unwrap();
    let boundary = &d.externs[0];
    assert_eq!(d.instances[boundary.instance.0].source.path, "Top.black");
    assert_eq!(boundary.connections.len(), 2);
    let pairs: BTreeMap<_, _> = boundary
        .connections
        .iter()
        .map(|(formal, actual)| {
            let SimExpr::Read(target) = actual else {
                panic!("expected a port signal")
            };
            (
                d.signal(*formal).source.path.as_str(),
                d.signal(target.signal).source.path.as_str(),
            )
        })
        .collect();
    assert_eq!(
        pairs,
        BTreeMap::from([("Top.black.i", "Top.i"), ("Top.black.o", "Top.y")])
    );
    let assertion = d.assertion(AssertId(0));
    let span = assertion.source.span();
    assert_eq!(&source[span.start..span.end], "assert(i)");
    assert_eq!(assertion.message, "assert failed");
    assert_eq!(
        d.resets[assertion.reset_mask.unwrap().0].polarity,
        HwResetPolarity::ActiveLow
    );
    assert!(matches!(assertion.condition, SimExpr::Read(_)));
    let port = d.signal(d.top_ports()[0].signal);
    assert!(source[port.source.span().start..port.source.span().end].contains("clk: in Clock"));
}

#[test]
fn static_boolean_selects_only_the_active_driver() {
    let src =
        "cell Choose<B: bool = true,>(x: in Bit, y: out Bit) { if B { y := x } else { y := 0 } }";
    let out = compile(&[("choose", src)]);
    let defaults = SimDesign::lower(&out, "Choose", &BTreeMap::new()).unwrap();
    assert!(matches!(defaults.comb[0].value, SimExpr::Read(_)));
    let other = SimDesign::lower(
        &out,
        "Choose",
        &BTreeMap::from([("B".into(), ParamValue::Bool(false))]),
    )
    .unwrap();
    assert_eq!(other.comb.len(), 1);
    assert_eq!(other.comb[0].value, SimExpr::Int(0));
    assert!(
        SimDesign::lower(
            &out,
            "Choose",
            &BTreeMap::from([("B".into(), ParamValue::Nat(0))])
        )
        .is_err()
    );
}

#[test]
fn all_reset_modes_bind_the_register_and_assertion_to_the_same_domain() {
    for (mode, expected_mode) in [("Sync", HwResetMode::Sync), ("Async", HwResetMode::Async)] {
        for (polarity, expected_polarity) in [
            ("ActiveHigh", HwResetPolarity::ActiveHigh),
            ("ActiveLow", HwResetPolarity::ActiveLow),
        ] {
            let src = format!(
                "cell R(clk: in Clock, rst: in Reset<{mode}, {polarity}>) -> y: Bit {{ reg q: Bit reset(rst, 1) next q := 0 y := q assert(q); }}"
            );
            let output = compile(&[("r", &src)]);
            let design = SimDesign::lower(&output, "R", &BTreeMap::new()).unwrap();
            let reg = design.register(RegId(0));
            let (id, initial) = reg.reset.as_ref().unwrap();
            let reset = &design.resets[id.0];
            assert_eq!(
                (reset.mode, reset.polarity),
                (expected_mode, expected_polarity)
            );
            assert_eq!(initial, &SimExpr::Int(1));
            let assertion = design.assertion(AssertId(0));
            assert_eq!(assertion.reset_mask, Some(*id));
            assert_eq!(assertion.clock, reg.clock);
        }
    }
}

#[test]
fn expansion_stack_retains_inplace_register_source() {
    let src = r#"
cell Delay(clk: in Clock, i: in Bit) -> o: Bit {
    reg previous: Bit
    next previous := i
    o := previous
}
cell Top(clk: in Clock, i: in Bit) -> o: Bit {
    let delayed = inplace Delay(clk: clk, i: i)
    o := delayed
}
"#;
    let d = SimDesign::lower(&compile(&[("top", src)]), "Top", &BTreeMap::new()).unwrap();
    let loc = &d.register(RegId(0)).source;
    assert_eq!(loc.name, "previous");
    assert_eq!(loc.path, "Top.delayed.previous");
    assert_eq!(loc.origin.expansion_stack()[0].callable(), "Delay");
}

#[test]
fn schedule_evaluates_the_child_before_consuming_its_output() {
    let src = "cell Pass(i: in Bit) -> o: Bit { o := i } cell Top(i: in Bit) -> o: Bit { let p = place Pass(i: i) o := p }";
    let d = SimDesign::lower(&compile(&[("top", src)]), "Top", &BTreeMap::new()).unwrap();
    let targets: Vec<_> = d
        .comb_eval_order
        .iter()
        .map(|id| d.signal(d.comb[id.0].target.signal).source.path.as_str())
        .collect();
    assert_eq!(targets, ["Top.p.i", "Top.p.o", "Top.p", "Top.o"]);
}

#[test]
fn required_generics_and_invalid_dimensions_return_errors() {
    let out = compile(&[(
        "s",
        "cell Word<W: nat>(i: in UInt<W>) -> o: UInt<W> { o := i }",
    )]);
    assert!(SimDesign::lower(&out, "Word", &BTreeMap::new()).is_err());
    for width in [0, u64::from(u32::MAX) + 1] {
        assert!(
            SimDesign::lower(
                &out,
                "Word",
                &BTreeMap::from([("W".into(), ParamValue::Nat(width))])
            )
            .is_err()
        );
    }
}

#[test]
fn high_impedance_driver_uses_the_shared_inout_net() {
    let src = "cell Pad(p: inout Bit) { p := z() } cell Top(p: inout Bit) { let child = place Pad(p: p) }";
    let out = compile(&[("pads", src)]);
    let d = SimDesign::lower(&out, "Top", &BTreeMap::new()).unwrap();
    let port = &d.top_ports()[0];
    assert_eq!(port.direction, HwDirection::InOut);
    let child = d.instances[1].ports[0].signal;
    assert_eq!(d.signal(child).alias.as_ref().unwrap().signal, port.signal);
    assert_eq!(d.comb.len(), 1);
    assert_eq!(d.comb[0].value, SimExpr::HighZ);
    assert_eq!(d.comb[0].target.signal, port.signal);
}

#[test]
fn already_unrolled_placements_retain_source_array_names() {
    let src = "cell Lane(i: in Bit, o: out Bit) { o := i } cell LaneArray(i: in [2] Bit, o: out [2] Bit) { let lanes = for lane in 0..2 { place Lane(i: i[lane], o: o[lane]) } }";
    let out = compile(&[("array", src)]);
    let design = SimDesign::lower(&out, "LaneArray", &BTreeMap::new()).unwrap();
    let family = design
        .families
        .iter()
        .find(|f| f.module.last().unwrap() == "Lane")
        .unwrap();
    assert_eq!(family.members.len(), 2);
    for (i, id) in family.members.iter().enumerate() {
        assert_eq!(
            design.instances[id.0].source.path,
            format!("LaneArray.lanes[{i}]")
        );
    }
}

#[test]
fn direct_port_slice_dependency_avoids_false_cycle_and_detects_real_cycle() {
    let source = r#"
cell Pick(i: in UInt<2>) -> o: Bit { o := i[0] }
cell Top(i: in Bit) -> o: Bit {
 signal a: UInt<2>
 let p = place Pick(i: a)
 a[0] := i
 a[1] := p
 o := a[1]
}
"#;
    let d = SimDesign::lower(&compile(&[("slice", source)]), "Top", &BTreeMap::new()).unwrap();
    let position = |path: &str, selectors: &[Selector]| {
        d.comb_eval_order
            .iter()
            .position(|id| {
                let target = &d.comb[id.0].target;
                d.signal(target.signal).source.path == path && target.selectors == selectors
            })
            .unwrap()
    };
    let low = position("Top.a", &[Selector::Slice { low: 0, high: 0 }]);
    let child = position("Top.p", &[]);
    let high = position("Top.a", &[Selector::Slice { low: 1, high: 1 }]);
    let output = position("Top.o", &[]);
    let input_low = position("Top.p.i", &[Selector::Slice { low: 0, high: 0 }]);
    let input_high = position("Top.p.i", &[Selector::Slice { low: 1, high: 1 }]);
    assert!(low < input_low && input_low < child && child < high && high < output);
    assert!(high < input_high);
    for (idx, bit) in [(input_low, 0), (input_high, 1)] {
        let assign = &d.comb[d.comb_eval_order[idx].0];
        let SimExpr::Read(t) = &assign.value else {
            panic!("expected a sliced copy")
        };
        assert_eq!(d.signal(t.signal).source.path, "Top.a");
        assert_eq!(
            t.selectors,
            [Selector::Slice {
                low: bit,
                high: bit
            }]
        );
    }
    let source = source.replace("o := i[0]", "o := i[1]");
    let err =
        SimDesign::lower(&compile(&[("slice", &source)]), "Top", &BTreeMap::new()).unwrap_err();
    assert!(matches!(err, SimError::CombinationalCycle { .. }));
}

#[test]
fn local_reset_signal_inherits_domain_and_polarity() {
    let source = r#"
cell Top(clk: in Clock, rst: in Reset<Async, ActiveLow>, i: in Bit) -> o: Bit {
 signal r: Reset<Async, ActiveLow>
 r := rst
 reg q: Bit reset(r, 0)
 next q := i
 o := q
}
"#;
    let d = SimDesign::lower(
        &compile(&[("local_reset", source)]),
        "Top",
        &BTreeMap::new(),
    )
    .unwrap();
    let reg = d.register(RegId(0));
    let reset = &d.resets[reg.reset.as_ref().unwrap().0.0];
    assert_eq!(d.signal(reset.signal).source.path, "Top.r");
    assert!(d.comb.iter().any(|a| a.target.signal == reset.signal
        && matches!(&a.value,SimExpr::Read(t) if d.signal(t.signal).source.path == "Top.rst")));
    assert_eq!(
        (reset.mode, reset.polarity),
        (HwResetMode::Async, HwResetPolarity::ActiveLow)
    );
}

#[test]
fn parameterized_clog2_selects_the_same_static_branch_as_sv() {
    let source = r#"
fn clog2(x: nat) -> nat {
    var n: nat = 0
    var p: nat = 1
    while p < x {
        p = p << 1
        n = n + 1
    }
    return n
}
cell Top<W: nat>(y: out Bit) {
    const IDX_W: nat = clog2(W)
    if IDX_W == 0 { y := 0 } else { y := 1 }
}
"#;
    let output = compile(&[("clog2", source)]);
    for (w, branch) in [(1, 0), (8, 1)] {
        let params = BTreeMap::from([("W".into(), ParamValue::Nat(w))]);
        let d = SimDesign::lower(&output, "Top", &params).unwrap();
        assert_eq!(d.signal(d.comb[0].target.signal).width, 1);
        assert_eq!(d.comb[0].value, SimExpr::Int(branch));
    }
    // Specialize a second top with a nonzero clog2-sized local declaration.
    let widths = source.replace(
        "if IDX_W == 0 { y := 0 } else { y := 1 }",
        "signal idx: UInt<IDX_W> := 0\n y := idx eq 0",
    );
    let output = compile(&[("widths", &widths)]);
    for (w, expected) in [(2, 1), (8, 3), (9, 4)] {
        let d = SimDesign::lower(
            &output,
            "Top",
            &BTreeMap::from([("W".into(), ParamValue::Nat(w))]),
        )
        .unwrap();
        assert_eq!(
            d.signals
                .iter()
                .find(|s| s.source.name == "idx")
                .unwrap()
                .width,
            expected
        );
    }
}

#[test]
fn inplace_instances_keep_source_paths_inside_constant_for() {
    let source = r#"
cell Leaf(i: in Bit) -> o: Bit { o := i }
cell Wrap(i: in Bit) -> o: Bit {
    let sub = place Leaf(i: i)
    o := sub
}
cell Top(i: in [2] Bit) -> o: [2] Bit {
    let wraps = for j in 0..2 {
        inplace Wrap(i: i[j])
    }
    o := i
}
"#;
    let design =
        SimDesign::lower(&compile(&[("inline_for", source)]), "Top", &BTreeMap::new()).unwrap();
    let leaves: Vec<_> = design
        .instances
        .iter()
        .filter(|instance| instance.source.name == "sub")
        .collect();
    assert_eq!(leaves.len(), 2);
    assert_eq!(leaves[0].source.path, "Top.wraps[0].sub");
    assert_eq!(leaves[1].source.path, "Top.wraps[1].sub");
    assert!(
        design
            .signals
            .iter()
            .any(|signal| signal.source.path == "Top.wraps[0].sub.i")
    );
    assert!(
        design
            .signals
            .iter()
            .any(|signal| signal.source.path == "Top.wraps[1].sub.i")
    );
}

#[test]
fn repeated_inline_wrappers_preserve_subcell_ports_and_state() {
    let src = "cell Leaf(clk: in Clock,i: in Bit) -> o: Bit { reg q: Bit next q := i o := q }
cell Wrap(clk: in Clock,i: in Bit) -> o: Bit { let sub = place Leaf(clk: clk,i: i) o := sub }
cell Top(clk: in Clock,i: in Bit) -> o: Bit { let a = inplace Wrap(clk: clk,i: i) let b = inplace Wrap(clk: clk,i: a) o := b }";
    let d = SimDesign::lower(&compile(&[("inline", src)]), "Top", &BTreeMap::new()).unwrap();
    for (index, prefix) in ["Top.a.sub", "Top.b.sub"].iter().enumerate() {
        let child = &d.instances[index + 1];
        assert_eq!(child.source.path, *prefix);
        assert_eq!(child.source.name, "sub");
        for p in &child.ports {
            assert_eq!(
                d.signal(p.signal).source.path,
                format!("{prefix}.{}", p.name)
            );
        }
        assert_eq!(d.regs[index].source.path, format!("{prefix}.q"));
    }
}

#[test]
fn empty_design_has_no_top_ports() {
    assert!(SimDesign::default().top_ports().is_empty());
}

#[test]
fn local_clock_and_reset_drive_sequential_sampling_and_assertions() {
    let src = "cell Top(c: in Bit,n: in Bit,i: in Bit) -> o: Bit {
        signal clk: Clock := c
        signal rst: Reset<Async,ActiveLow> := n
        reg q: Bit reset(rst,0)
        next q := i
        assert(i)
        o := q
    }";
    let d = SimDesign::lower(&compile(&[("local", src)]), "Top", &BTreeMap::new()).unwrap();
    let r = &d.regs[0];
    let id = r.reset.as_ref().unwrap().0;
    assert_eq!(d.signal(d.clocks[r.clock.0].signal).source.path, "Top.clk");
    assert_eq!(d.signal(d.resets[id.0].signal).source.path, "Top.rst");
    assert_eq!(d.assertions[0].reset_mask, Some(id));
    assert_eq!(d.assertions[0].clock, r.clock);
}
