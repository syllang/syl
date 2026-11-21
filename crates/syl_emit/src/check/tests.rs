use super::*;

fn empty_module(name: &str) -> SvModule {
    SvModule::new(name, Vec::new(), Vec::new(), Vec::new())
}

#[test]
fn rejects_non_system_function_calls() {
    let err = SvBackendValidator::new(&SvDesign::new(vec![SvModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![SvItem::Assign {
            lhs: SvExpr::Ident("y".to_string()),
            rhs: SvExpr::Call {
                name: "helper".to_string(),
                args: vec![SvExpr::Int(1)],
            },
        }],
    )]))
    .validate()
    .expect_err("generated SV must not reference functions that the backend did not emit");

    assert!(matches!(
        err,
        CompileError::Verilog {
            kind: VerilogError::UnsupportedFunctionCall { module, name }
        } if module == "Top" && name == "helper"
    ));
}

#[test]
fn source_validator_rejects_unmatched_endmodule() {
    let err = SvSourceValidator::new()
        .validate("endmodule\n")
        .expect_err("source validation must reject closing an unopened module");

    assert!(matches!(
        err,
        CompileError::Verilog {
            kind: VerilogError::UnmatchedEndModule { line }
        } if line == 1
    ));
}

#[test]
fn source_validator_accepts_emitted_empty_module() {
    let source = SvDesign::new(vec![empty_module("Top")]).emit_text();

    SvSourceValidator::new()
        .validate(&source)
        .expect("well-formed emitted module text should satisfy backend-local source checks");
}
