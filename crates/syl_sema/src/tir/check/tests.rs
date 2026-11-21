use crate::HirResolver;
use crate::{
    MiddleCompiler,
    tir::{BindingRef, TypePhaseChecker},
};
use syl_span::Span;
use syl_syntax::{AstFile, Block, CallableItem, Expr, Item, MapItem, Stmt, TypeExpr};

#[test]
fn check_tir_partial_keeps_valid_item_hover_after_other_item_error() {
    let valid_expr_span = Span::new(110, 111);
    let error_span = Span::new(210, 211);
    let files = vec![AstFile::new(vec![
        valid_map("Good", valid_expr_span),
        bad_module("Bad", error_span),
    ])];
    let hir = MiddleCompiler::new()
        .session(&files)
        .resolve_hir()
        .expect("HIR should resolve before TIR recovery");
    let output = hir.check_tir_partial();

    assert!(!output.diagnostics().is_empty());
    let stage = output
        .partial_stage()
        .expect("recoverable TIR errors should keep collected typed facts");
    let hover = stage
        .hover_at(valid_expr_span)
        .expect("valid item expression should still have typed hover");

    assert_eq!(hover.text(), "Comb nat");
}

#[test]
fn check_tir_partial_returns_stage_with_diagnostics_for_recoverable_error() {
    let error_span = Span::new(30, 31);
    let files = vec![AstFile::new(vec![
        valid_typed_map("Good", Span::new(70, 71)),
        bad_module("Bad", error_span),
    ])];
    let hir = MiddleCompiler::new()
        .session(&files)
        .resolve_hir()
        .expect("HIR should resolve before TIR recovery");
    let output = hir.check_tir_partial();

    assert!(!output.diagnostics().is_empty());
    assert!(
        output.stage().is_some(),
        "recoverable TIR diagnostics must not imply a missing stage"
    );
}

#[test]
fn check_tir_partial_keeps_valid_fact_after_same_item_error() {
    let error_span = Span::new(30, 31);
    let valid_span = Span::new(60, 61);
    let files = vec![AstFile::new(vec![Item::Module(
        CallableItem::builder(
            "Mixed".to_string(),
            Block::new(
                vec![
                    Stmt::ElabIf {
                        cond: Expr::Int(1, error_span),
                        then_block: Block::new(Vec::new(), None, Span::new(32, 34)),
                        else_block: None,
                        span: Span::new(20, 34),
                    },
                    Stmt::Let {
                        name: "ok".to_string(),
                        ty: None,
                        value: Some(Expr::Int(7, valid_span)),
                        span: Span::new(50, 70),
                    },
                ],
                None,
                Span::new(10, 80),
            ),
        )
        .span(Span::new(0, 80))
        .build(),
    )])];
    let hir = MiddleCompiler::new()
        .session(&files)
        .resolve_hir()
        .expect("HIR should resolve before TIR recovery");
    let output = hir.check_tir_partial();

    assert!(!output.diagnostics().is_empty());
    let stage = output
        .partial_stage()
        .expect("same-item recovery should keep partial TIR facts");
    let hover = stage
        .hover_at(valid_span)
        .expect("later valid let expression should still have typed hover");

    assert_eq!(hover.text(), "Hardware nat");
}

#[test]
fn check_output_keeps_recorded_binding_types_with_diagnostics() {
    let error_span = Span::new(80, 81);
    let files = vec![AstFile::new(vec![
        valid_typed_map("Good", Span::new(40, 41)),
        bad_module("Bad", error_span),
    ])];
    let hir = HirResolver::new(&files)
        .resolve()
        .expect("HIR should resolve before TIR recovery");
    let good = hir
        .defs()
        .iter()
        .find(|def| def.name == "Good")
        .expect("valid map def should exist");
    let param = hir
        .locals()
        .iter()
        .find(|local| local.owner == good.id && local.name == "x")
        .expect("valid map param should exist");
    let param_id = param.id;
    let output = TypePhaseChecker::new(std::sync::Arc::new(hir)).check_output();

    assert!(!output.diagnostics().is_empty());
    let tir = output
        .stage()
        .expect("TIR output should keep partial facts after recoverable errors");
    assert_eq!(
        tir.binding_type_label(BindingRef::Local(param_id))
            .as_deref(),
        Some("nat")
    );
}

fn valid_map(name: &str, expr_span: Span) -> Item {
    Item::Map(
        MapItem::builder(name.to_string(), Expr::Int(1, expr_span))
            .span(Span::new(100, 120))
            .build(),
    )
}

fn valid_typed_map(name: &str, expr_span: Span) -> Item {
    Item::Map(
        MapItem::builder(name.to_string(), Expr::Ident("x".to_string(), expr_span))
            .params(vec![syl_syntax::Param::new(
                "x".to_string(),
                None,
                TypeExpr::Path(vec!["nat".to_string()], Span::new(12, 15)),
                Span::new(10, 15),
            )])
            .span(Span::new(0, 50))
            .build(),
    )
}

fn bad_module(name: &str, cond_span: Span) -> Item {
    Item::Module(
        CallableItem::builder(
            name.to_string(),
            Block::new(
                vec![Stmt::ElabIf {
                    cond: Expr::Int(1, cond_span),
                    then_block: Block::new(Vec::new(), None, Span::new(220, 222)),
                    else_block: None,
                    span: Span::new(200, 222),
                }],
                None,
                Span::new(190, 230),
            ),
        )
        .span(Span::new(180, 240))
        .build(),
    )
}
