use super::DriverBound;
use crate::eir::{EirBinaryOp, EirExpr};

#[test]
fn symbolic_extent_ignores_neutral_one_and_parentheses() {
    let bound = DriverBound::new("((N) * (1))");

    assert!(bound.is_symbolic_extent("N"));
}

#[test]
fn symbolic_extent_times_accepts_commuted_product() {
    let total = DriverBound::new("(W + 1) * N");
    let width = DriverBound::new("W + 1");

    assert!(total.is_symbolic_extent_times("N", &width));
}

#[test]
fn symbolic_extent_times_normalizes_numeric_width() {
    let total = DriverBound::new("(N)*(8)");
    let width = DriverBound::new("8");

    assert!(total.is_symbolic_extent_times("N", &width));
}

#[test]
fn eir_expr_constructs_bound_formula_without_source_parsing() {
    let width = EirExpr::binary(EirBinaryOp::Add, EirExpr::ident("W"), EirExpr::Int(1));
    let total = DriverBound::from_eir_expr(&EirExpr::binary(
        EirBinaryOp::Mul,
        EirExpr::ident("N"),
        width.clone(),
    ));

    assert!(total.is_symbolic_extent_times("N", &DriverBound::from_eir_expr(&width)));
}
