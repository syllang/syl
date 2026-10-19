use super::DriverLoopGuard;
use crate::{
    driver::place::{DriverBound, DriverExpr},
    eir::{EirGuard, EirGuardFrame},
};
use syl_span::Span;

#[test]
fn single_loop_bounds_accept_symbolic_part_array() {
    let frame = EirGuardFrame::loop_frame("gen_i", "i", "0", "N", Span::default());
    let guard = EirGuard::from_frames(&[frame]);
    let bounds = DriverLoopGuard::new(&guard)
        .single_loop_bounds()
        .expect("single loop guard must expose symbolic bounds");

    assert!(bounds.index_matches(&DriverExpr::Ident("i".to_string())));
    assert!(bounds.covers_part_array(&DriverBound::new("N * 8"), &DriverBound::new("8")));
}

#[test]
fn single_loop_context_keeps_branch_residual() {
    let frames = [
        EirGuardFrame::loop_frame("gen_i", "i", "0", "N", Span::default()),
        EirGuardFrame::if_then("branch", Span::default()),
    ];
    let guard = EirGuard::from_frames(&frames);
    let context = DriverLoopGuard::new(&guard)
        .single_loop_context()
        .expect("one loop plus branch context should expose symbolic bounds");

    assert!(
        context
            .bounds()
            .index_matches(&DriverExpr::Ident("i".to_string()))
    );
    assert_eq!(context.into_residual_guard().frames().len(), 1);
}

#[test]
fn single_loop_bounds_reject_branch_context() {
    let frame = EirGuardFrame::if_then("branch", Span::default());
    let guard = EirGuard::from_frames(&[frame]);

    assert!(DriverLoopGuard::new(&guard).single_loop_bounds().is_none());
}
