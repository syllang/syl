use super::{CoverageRoot, SymbolicLoopCoverage};
use crate::{
    driver::place::{DriverBound, DriverExpr, DriverPlace},
    eir::{EirBinaryOp, EirBound, EirExpr, EirGuard, EirGuardFrame},
};
use syl_span::Span;

#[test]
fn symbolic_loop_coverage_requires_branch_residual_coverage() {
    let port_width = EirBound::new(
        "N * 8",
        EirExpr::binary(EirBinaryOp::Mul, EirExpr::ident("N"), EirExpr::Int(8)),
    );
    let mut coverage = SymbolicLoopCoverage::new("out", &port_width, CoverageRoot::new(None));
    let place = DriverPlace::IndexedPartSelect {
        base: Box::new(DriverPlace::test_object(0, "out")),
        index: DriverExpr::Ident("i".to_string()),
        width: DriverBound::new("8"),
    };
    let loop_frame = EirGuardFrame::loop_frame("gen_i", "i", "0", "N", Span::default());
    let then_guard = EirGuard::from_frames(&[
        loop_frame.clone(),
        EirGuardFrame::if_then("branch", Span::default()),
    ]);
    let else_guard = EirGuard::from_frames(&[
        loop_frame,
        EirGuardFrame::if_else("branch", Span::default()),
    ]);

    coverage.add_drive(&place, &then_guard);
    assert!(!coverage.proves_complete());

    coverage.add_drive(&place, &else_guard);
    assert!(coverage.proves_complete());
}
