use super::*;
use crate::{
    HwConnection, HwDirection, HwExpr, HwInstance, HwItem, HwOrigin, HwPort, ParametricHwItem,
};
use syl_span::SourceId;

fn origin() -> HwOrigin {
    HwOrigin::new(SourceId::new(0), 0, 0, Vec::new())
}

fn item(item: HwItem) -> ParametricHwItem {
    ParametricHwItem::core(item, origin())
}

fn module(name: &str, ports: Vec<HwPort>, items: Vec<ParametricHwItem>) -> ParametricHwModule {
    ParametricHwModule::new(name, Vec::new(), ports, items)
}

#[test]
fn rejects_duplicate_module_names() {
    let report = HwValidator::new()
        .validate(&ParametricHwDesign::new(vec![
            module("Top", Vec::new(), Vec::new()),
            module("Top", Vec::new(), Vec::new()),
        ]))
        .expect_err("duplicate module names must be rejected before backend emission");

    assert_eq!(
        report.diagnostics(),
        &[HwValidationDiagnostic::DuplicateModule {
            name: "Top".to_string(),
        }]
    );
}

#[test]
fn rejects_duplicate_ports_and_unknown_reference() {
    let report = HwValidator::new()
        .validate(&ParametricHwDesign::new(vec![module(
            "Top",
            vec![
                HwPort::new(HwDirection::In, "1", "x"),
                HwPort::new(HwDirection::Out, "1", "x"),
            ],
            vec![item(HwItem::ContinuousDrive {
                lhs: HwExpr::Ident("x".to_string()),
                rhs: HwExpr::Ident("missing".to_string()),
            })],
        )]))
        .expect_err("duplicate ports and dangling identifiers must stay in HW validation");

    assert_eq!(
        report.diagnostics(),
        &[
            HwValidationDiagnostic::DuplicateBinding {
                module: "Top".to_string(),
                kind: HwBindingKind::Port,
                name: "x".to_string(),
            },
            HwValidationDiagnostic::UnknownReference {
                module: "Top".to_string(),
                name: "missing".to_string(),
            },
        ]
    );
}

#[test]
fn rejects_unknown_instance_target_and_port_binding() {
    let report = HwValidator::new()
        .validate(&ParametricHwDesign::new(vec![
            module(
                "Top",
                vec![HwPort::new(HwDirection::In, "1", "x")],
                vec![item(HwItem::Instance(HwInstance::new(
                    "Missing",
                    Vec::new(),
                    "u_missing",
                    vec![HwConnection::new("y", HwExpr::Ident("x".to_string()))],
                )))],
            ),
            module("Other", Vec::new(), Vec::new()),
        ]))
        .expect_err("instances must target a known module before hitting an emitter");

    assert_eq!(
        report.diagnostics(),
        &[HwValidationDiagnostic::UnknownInstanceTarget {
            module: "Top".to_string(),
            instance: "u_missing".to_string(),
            target: "Missing".to_string(),
        }]
    );
}

#[test]
fn rejects_unknown_instance_formal_and_duplicate_binding() {
    let report = HwValidator::new()
        .validate(&ParametricHwDesign::new(vec![
            module(
                "Child",
                vec![HwPort::new(HwDirection::In, "1", "x")],
                Vec::new(),
            ),
            module(
                "Top",
                vec![HwPort::new(HwDirection::In, "1", "x")],
                vec![item(HwItem::Instance(HwInstance::new(
                    "Child",
                    Vec::new(),
                    "u_child",
                    vec![
                        HwConnection::new("missing", HwExpr::Ident("x".to_string())),
                        HwConnection::new("missing", HwExpr::Ident("x".to_string())),
                    ],
                )))],
            ),
        ]))
        .expect_err("instance bindings must resolve against the target module interface");

    assert_eq!(
        report.diagnostics(),
        &[
            HwValidationDiagnostic::UnknownInstancePort {
                module: "Top".to_string(),
                instance: "u_child".to_string(),
                target: "Child".to_string(),
                name: "missing".to_string(),
            },
            HwValidationDiagnostic::DuplicateInstanceBinding {
                module: "Top".to_string(),
                instance: "u_child".to_string(),
                kind: HwBindingKind::Port,
                name: "missing".to_string(),
            },
        ]
    );
}

#[test]
fn rejects_invalid_identifiers_and_blank_widths() {
    let report = HwValidator::new()
        .validate(&ParametricHwDesign::new(vec![module(
            "1bad",
            vec![HwPort::new(HwDirection::In, " ", "bad port")],
            vec![item(HwItem::SignalDecl {
                width: String::new(),
                name: "tmp".to_string(),
            })],
        )]))
        .expect_err("name and width sanity must fail before backend-specific lowering");

    assert_eq!(
        report.diagnostics(),
        &[
            HwValidationDiagnostic::InvalidIdentifier {
                module: None,
                kind: HwBindingKind::Module,
                name: "1bad".to_string(),
            },
            HwValidationDiagnostic::InvalidIdentifier {
                module: Some("1bad".to_string()),
                kind: HwBindingKind::Port,
                name: "bad port".to_string(),
            },
            HwValidationDiagnostic::InvalidWidth {
                module: "1bad".to_string(),
                kind: HwBindingKind::Port,
                name: "bad port".to_string(),
                width: " ".to_string(),
            },
            HwValidationDiagnostic::InvalidWidth {
                module: "1bad".to_string(),
                kind: HwBindingKind::Signal,
                name: "tmp".to_string(),
                width: String::new(),
            },
        ]
    );
}

#[test]
fn normalizer_returns_borrowed_valid_design() {
    let design = ParametricHwDesign::new(vec![module(
        "Top",
        vec![HwPort::new(HwDirection::Out, "1", "y")],
        vec![item(HwItem::ContinuousDrive {
            lhs: HwExpr::Ident("y".to_string()),
            rhs: HwExpr::Bool(false),
        })],
    )]);

    let normalized = HwNormalizer::new()
        .normalize(&design)
        .expect("valid HW IR should normalize without backend coupling");

    assert_eq!(normalized.debug_dump(), design.debug_dump());
    assert_eq!(normalized.modules().len(), 1);
}
