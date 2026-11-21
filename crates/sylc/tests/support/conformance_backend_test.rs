use std::{fs, path::Path};

use syl_emit::{CompileError, VerilogError};
use syl_hw::{
    HwDirection, HwExpr, HwInstance, HwItem, HwOrigin, HwPort, HwValidationDiagnostic,
    ParametricHwDesign, ParametricHwItem, ParametricHwModule,
};
use syl_span::SourceId;

// This small fixture format constructs intentionally invalid HWIR, which a valid
// Syl frontend cannot produce. See conformance/README.md for its grammar.
pub fn read_design(path: &Path) -> ParametricHwDesign {
    let source = fs::read_to_string(path).expect("backend fixture must be readable");
    let mut modules = Vec::new();
    let mut name = None;
    let mut ports = Vec::new();
    let mut items = Vec::new();
    for (index, line) in source.lines().enumerate() {
        let words: Vec<_> = line.split_whitespace().collect();
        if words.is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        match words.as_slice() {
            ["module", next_name] => {
                assert!(
                    name.is_none(),
                    "{}:{} missing end",
                    path.display(),
                    index + 1
                );
                name = Some(*next_name);
            }
            ["end"] => modules.push(ParametricHwModule::new(
                name.take().expect("end must follow module"),
                Vec::new(),
                std::mem::take(&mut ports),
                std::mem::take(&mut items),
            )),
            _ => {
                assert!(
                    name.is_some(),
                    "{}:{} item outside module",
                    path.display(),
                    index + 1
                );
                let item = match words.as_slice() {
                    ["input", width, port] | ["output", width, port] => {
                        let direction = if words[0] == "input" {
                            HwDirection::In
                        } else {
                            HwDirection::Out
                        };
                        ports.push(HwPort::new(direction, *width, *port));
                        continue;
                    }
                    ["drive", lhs, rhs] => HwItem::ContinuousDrive {
                        lhs: HwExpr::Ident((*lhs).into()),
                        rhs: HwExpr::Ident((*rhs).into()),
                    },
                    ["call", lhs, callee] => HwItem::ContinuousDrive {
                        lhs: HwExpr::Ident((*lhs).into()),
                        rhs: HwExpr::Call {
                            name: (*callee).into(),
                            args: Vec::new(),
                        },
                    },
                    ["instance", target, instance] => HwItem::Instance(HwInstance::new(
                        *target,
                        Vec::new(),
                        *instance,
                        Vec::new(),
                    )),
                    _ => panic!(
                        "{}:{} invalid fixture directive: {line}",
                        path.display(),
                        index + 1
                    ),
                };
                items.push(ParametricHwItem::core(
                    item,
                    HwOrigin::new(SourceId::new(0), 0, 0, Vec::new()),
                ));
            }
        }
    }
    assert!(name.is_none(), "{} missing final end", path.display());
    assert!(!modules.is_empty(), "{} needs a module", path.display());
    ParametricHwDesign::new(modules)
}

// HW/backend errors expose enum variants rather than Diagnostic codes. Keep
// the fixture vocabulary tied to those variants, never to their display text.
pub fn error_codes(error: &CompileError) -> Vec<String> {
    match error {
        CompileError::InvalidHwir { report } => report
            .diagnostics()
            .iter()
            .map(|diagnostic| {
                match diagnostic {
                    HwValidationDiagnostic::DuplicateModule { .. } => "E_HW_DUPLICATE_MODULE",
                    HwValidationDiagnostic::DuplicateBinding { .. } => "E_HW_DUPLICATE_BINDING",
                    HwValidationDiagnostic::UnknownReference { .. } => "E_HW_UNKNOWN_REFERENCE",
                    HwValidationDiagnostic::UnknownInstanceTarget { .. } => {
                        "E_HW_UNKNOWN_INSTANCE_TARGET"
                    }
                    HwValidationDiagnostic::InvalidIdentifier { .. } => "E_HW_INVALID_IDENTIFIER",
                    other => panic!("unmapped structured HW diagnostic: {other:?}"),
                }
                .to_string()
            })
            .collect(),
        CompileError::Verilog {
            kind: VerilogError::UnsupportedFunctionCall { .. },
        } => {
            vec!["E_SV_UNSUPPORTED_FUNCTION_CALL".to_string()]
        }
        other => panic!("unexpected structured backend error: {other:?}"),
    }
}
