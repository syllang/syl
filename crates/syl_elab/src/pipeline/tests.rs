use super::MiddleCompiler;
use syl_span::Span;
use syl_syntax::{AstFile, Block, CallableItem, Expr, Item, Param, ParamDirection, Stmt, TypeExpr};

#[test]
fn diagnostics_stop_after_tir_errors() {
    let files = vec![AstFile::new(vec![
        Item::Module(
            CallableItem::builder(
                "Good".to_string(),
                Block::new(Vec::new(), None, Span::new(10, 20)),
            )
            .params(vec![Param::new(
                "y".to_string(),
                Some(ParamDirection::Out),
                TypeExpr::Path(vec!["Bit".to_string()], Span::new(60, 61)),
                Span::new(60, 61),
            )])
            .span(Span::new(0, 20))
            .build(),
        ),
        Item::Module(
            CallableItem::builder(
                "Bad".to_string(),
                Block::new(
                    vec![Stmt::ElabIf {
                        cond: Expr::Int(1, Span::new(30, 31)),
                        then_block: Block::new(Vec::new(), None, Span::new(32, 34)),
                        else_block: None,
                        span: Span::new(20, 34),
                    }],
                    None,
                    Span::new(10, 40),
                ),
            )
            .span(Span::new(0, 40))
            .build(),
        ),
    ])];
    let session = MiddleCompiler::new().session(&files);
    assert_eq!(
        session.diagnostics(),
        session
            .resolve_hir()
            .expect("HIR should resolve")
            .check_tir_partial()
            .diagnostics()
    );
}
