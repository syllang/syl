use std::cell::Cell;

use syl_elab::HardwareCompiler;
use syl_sema::{SemanticCompiler, TirAnalysis};
use syl_syntax::SourceParser;

fn tir_from_source(source: &str) -> TirAnalysis {
    let file = SourceParser::new(source)
        .parse_file()
        .expect("test source must parse");
    SemanticCompiler::new()
        .session(&[file])
        .resolve_hir()
        .expect("test source must resolve to HIR")
        .check_tir()
        .expect("test source must resolve to TIR")
}

#[test]
fn output_for_tir_with_token_stops_between_passes() {
    let tir = tir_from_source("cell Top(y: out Bit) { y := 1 }\n");
    let checks = Cell::new(0);
    let token = || {
        let next = checks.get() + 1;
        checks.set(next);
        next > 1
    };
    let output = HardwareCompiler::new().output_for_tir_with_token(&tir, &token);

    assert!(output.const_mir().is_some());
    assert!(output.map_ir().is_none());
    assert!(output.eir_build().is_none());
    assert!(output.hwir().is_none());
}

#[test]
fn compile_tir_with_token_returns_none_after_cancellation() {
    let tir = tir_from_source("cell Top(y: out Bit) { y := 1 }\n");
    let checks = Cell::new(0);
    let token = || {
        let next = checks.get() + 1;
        checks.set(next);
        next > 1
    };

    let output = HardwareCompiler::new()
        .compile_tir_with_token(&tir, &token)
        .expect("cancellation should not turn into a lowering error");

    assert!(output.is_none());
}
