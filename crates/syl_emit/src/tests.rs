use super::*;
use syl_hw::{HwDirection, HwParam, HwPort, ParametricHwModule};

#[test]
fn emits_module_param_and_port_docs_as_verilog_comments() {
    let design = ParametricHwDesign::new(vec![
        ParametricHwModule::new(
            "Top",
            vec![HwParam::new("N", "4").with_doc(Some("Width parameter.".to_string()))],
            vec![HwPort::new(HwDirection::In, "N", "x").with_doc(Some("Input bits.".to_string()))],
            Vec::new(),
        )
        .with_doc(Some("Top module.".to_string())),
    ]);

    let sv = SystemVerilogBackend::new()
        .emit(&design)
        .expect("documented HWIR should emit");

    assert!(sv.contains("// Top module.\nmodule Top"));
    assert!(sv.contains("    // Width parameter.\n    parameter N = 4"));
    assert!(sv.contains("    // Input bits.\n    input [(N)-1:0] x"));
}

#[test]
fn emits_bool_literals_legally_in_module_params() {
    let design = ParametricHwDesign::new(vec![ParametricHwModule::new(
        "Top",
        vec![
            HwParam::new("ENABLE", "true"),
            HwParam::new("DISABLE", "false"),
        ],
        Vec::new(),
        Vec::new(),
    )]);

    let sv = SystemVerilogBackend::new()
        .emit(&design)
        .expect("bool-valued params should emit as legal SystemVerilog literals");

    assert!(sv.contains("parameter ENABLE = 1'b1"));
    assert!(sv.contains("parameter DISABLE = 1'b0"));
}

#[test]
fn files_offer_canonical_paths_and_unresolved_source_ranges() {
    let origin = syl_hw::HwOrigin::new(Default::default(), 3, 9, vec![]);
    let index = origin.source_index();
    let module = ParametricHwModule::new("Top", vec![], vec![], vec![])
        .with_canonical_path(vec!["demo".into(), "Top".into()])
        .with_origin(Some(origin));
    let design = ParametricHwDesign::new(vec![module]);
    let backend = SystemVerilogBackend::new();
    let bundle = backend
        .emit_files_with_options(
            &design,
            &EmitOptions::new()
                .with_source_comments(true)
                .with_header_comment(false),
            None,
        )
        .unwrap();
    assert_eq!(bundle.filelist_lines(), ["Top.sv"]);
    let file = &bundle.files()[0];
    assert_eq!(file.module_name(), "Top");
    assert_eq!(file.canonical_path(), ["demo", "Top"]);
    assert!(file.content().starts_with(&format!(
        "// syl: demo.Top\n// source: source#{index}:3..9\nmodule Top"
    )));
    assert_eq!(
        backend.emit_files(&design).unwrap().files()[0].content(),
        backend.emit(&design).unwrap()
    );
}

fn core(item: syl_hw::HwItem) -> syl_hw::ParametricHwItem {
    syl_hw::ParametricHwItem::core(
        item,
        syl_hw::HwOrigin::new(Default::default(), 0, 0, vec![]),
    )
}

#[test]
fn emits_all_reset_mode_and_polarity_combinations_exactly() {
    use syl_hw::{HwExpr as E, HwItem, HwReset, HwResetMode as M, HwResetPolarity as P};
    for (mode, polarity, edge, condition) in [
        (M::Sync, P::ActiveHigh, "clk", "rst"),
        (M::Sync, P::ActiveLow, "clk", "!rst"),
        (M::Async, P::ActiveHigh, "clk or posedge rst", "rst"),
        (M::Async, P::ActiveLow, "clk or negedge rst", "!rst"),
    ] {
        let reset = HwReset::new(E::Ident("rst".into()), E::Int(0))
            .with_mode(mode)
            .with_polarity(polarity);
        let design = ParametricHwDesign::new(vec![ParametricHwModule::new(
            "Top",
            vec![],
            vec![
                HwPort::new(HwDirection::In, "1", "clk"),
                HwPort::new(HwDirection::In, "1", "rst"),
            ],
            vec![
                core(HwItem::StorageDecl {
                    width: "1".into(),
                    name: "q".into(),
                }),
                core(HwItem::ClockedStorage {
                    clock: E::Ident("clk".into()),
                    target: E::Ident("q".into()),
                    reset: Some(reset),
                    next: E::Int(1),
                }),
            ],
        )]);
        let sv = SystemVerilogBackend::new().emit(&design).unwrap();
        assert!(sv.contains(&format!("  always @(posedge {edge}) begin\n    if ({condition}) q <= 0;\n    else q <= 1;\n  end\n")), "{sv}");
    }
}

#[test]
fn emits_synthesizable_unpacked_memories_and_enabled_ports() {
    use syl_hw::{HwExpr as E, HwItem};
    let id = |s: &str| E::Ident(s.into());
    let design = ParametricHwDesign::new(vec![ParametricHwModule::new(
        "Ram",
        vec![HwParam::new("DEPTH", "16")],
        vec![
            HwPort::new(HwDirection::In, "1", "clk"),
            HwPort::new(HwDirection::In, "1", "en"),
            HwPort::new(HwDirection::In, "4", "addr"),
            HwPort::new(HwDirection::In, "8", "data"),
        ],
        vec![
            core(HwItem::MemoryDecl {
                name: "storage".into(),
                elem_width: "8".into(),
                depth: "16".into(),
            }),
            core(HwItem::MemoryDecl {
                name: "param_storage".into(),
                elem_width: "8".into(),
                depth: "DEPTH".into(),
            }),
            core(HwItem::StorageDecl {
                name: "rd".into(),
                width: "8".into(),
            }),
            core(HwItem::MemoryWrite {
                clock: id("clk"),
                memory: "storage".into(),
                enable: id("en"),
                addr: id("addr"),
                data: id("data"),
            }),
            core(HwItem::MemorySyncRead {
                clock: id("clk"),
                memory: "storage".into(),
                enable: id("en"),
                addr: id("addr"),
                target: id("rd"),
            }),
        ],
    )]);
    let sv = SystemVerilogBackend::new().emit(&design).unwrap();
    assert!(sv.contains("reg [7:0] storage [0:15];"), "{sv}");
    assert!(
        sv.contains("reg [7:0] param_storage [0:(DEPTH)-1];"),
        "{sv}"
    );
    assert!(
        sv.contains("always @(posedge clk) begin\n    if (en) storage[addr] <= data;\n  end"),
        "{sv}"
    );
    assert!(
        sv.contains("always @(posedge clk) begin\n    if (en) rd <= storage[addr];\n  end"),
        "{sv}"
    );
}
