use super::*;
use crate::{
    eir::{
        EirDesign, EirDesignComposer, EirDriveKind, EirFactCollector, EirItem, EirModule,
        EirRawDesign,
    },
    eir::{EirGuard, EirOrigin, EirPlace},
};
use std::sync::Arc;
use syl_sema::OpaqueSummaryTable;
use syl_span::{SourceId, Span};

fn validated_design(modules: Vec<EirModule>) -> Result<EirDesign, CompileError> {
    let raw = Arc::new(EirRawDesign::new(modules));
    EirValidator::new(raw.modules()).validate()?;
    let facts = Arc::new(EirFactCollector::collect(
        raw.modules(),
        &OpaqueSummaryTable::new(),
    )?);
    Ok(EirDesignComposer::compose(raw, facts))
}

#[test]
fn rejects_unsupported_expr_before_collecting_facts() {
    let span = Span::new_in(SourceId::new(0), 10, 20);
    let origin = EirOrigin::new(span, Vec::new());
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![EirItem::Drive {
            lhs: EirPlace::Ident("y".to_string()),
            rhs: EirExpr::unsupported("test unsupported"),
            reads: Vec::new(),
            origin,
        }],
    );

    let validation = {
        let raw = EirRawDesign::new(vec![module]);
        EirValidator::new(raw.modules()).validate()
    };
    let error = match validation {
        Ok(_) => panic!("unsupported EIR expression must be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.diagnostic().span, span);
}

#[test]
fn rejects_unsupported_expr_inside_place_projection() {
    let span = Span::new_in(SourceId::new(1), 30, 40);
    let origin = EirOrigin::new(span, Vec::new());
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![EirItem::ClockedStorage {
            clock: EirExpr::ident("clk"),
            target: EirPlace::Index {
                base: Box::new(EirPlace::Ident("r".to_string())),
                index: EirExpr::unsupported("bad index"),
            },
            reset: None,
            next: EirExpr::ident("r"),
            reads: Vec::new(),
            origin,
        }],
    );

    let validation = {
        let raw = EirRawDesign::new(vec![module]);
        EirValidator::new(raw.modules()).validate()
    };
    let error = match validation {
        Ok(_) => panic!("unsupported EIR place index must be rejected"),
        Err(error) => error,
    };

    assert_eq!(error.diagnostic().span, span);
}

#[test]
fn accepts_supported_exprs_and_collects_facts() {
    let span = Span::new_in(SourceId::new(2), 50, 60);
    let origin = EirOrigin::new(span, Vec::new());
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![EirItem::Drive {
            lhs: EirPlace::Ident("y".to_string()),
            rhs: EirExpr::ident("x"),
            reads: vec![EirExpr::ident("x")],
            origin,
        }],
    );

    let design = match validated_design(vec![module]) {
        Ok(design) => design,
        Err(error) => panic!("supported EIR must validate: {error}"),
    };

    assert_eq!(design.drives().len(), 1);
    assert!(matches!(
        design.drives()[0].kind(),
        EirDriveKind::Continuous
    ));
    assert_eq!(design.drives()[0].guard(), &EirGuard::root());
    assert_eq!(design.reads().len(), 1);
}
