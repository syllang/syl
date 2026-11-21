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
