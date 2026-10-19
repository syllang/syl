use super::*;
use crate::{
    LoweringError,
    eir::{
        EirDesign, EirDesignComposer, EirFactCollector, EirItem, EirModule, EirRawDesign,
        EirValidator,
    },
    eir::{EirExpr, EirOrigin, EirPlace},
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
fn rejects_unresolved_drive_root() {
    let span = Span::new_in(SourceId::new(0), 4, 12);
    let origin = EirOrigin::new(span, Vec::new());
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![EirItem::Drive {
            lhs: EirPlace::Ident("missing".to_string()),
            rhs: EirExpr::Int(0),
            reads: Vec::new(),
            origin,
        }],
    );
    let design = match validated_design(vec![module]) {
        Ok(design) => design,
        Err(error) => panic!("test EIR should pass structural validation: {error}"),
    };

    let errors = match DriverFactsCollector::new(&design).collect() {
        Ok(_) => panic!("driver facts pass must reject unresolved drive roots"),
        Err(errors) => errors,
    };

    assert!(matches!(
        errors.first(),
        Some(CompileError::Lowering { kind, .. })
            if matches!(
                kind.as_ref(),
                LoweringError::Driver(DriverError::UnknownHardwareObject { module, name })
                    if module == "Top" && name == "missing"
            )
    ));
    assert_eq!(errors[0].diagnostic().span, span);
}

#[test]
fn next_drive_facts_keep_effect_summary_separate_from_target_name() {
    let span = Span::new_in(SourceId::new(1), 20, 30);
    let origin = EirOrigin::new(span, Vec::new());
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![
            EirItem::Storage {
                width: "1".into(),
                name: "state".to_string(),
                origin: origin.clone(),
            },
            EirItem::ClockedStorage {
                clock: EirExpr::ident("clk"),
                target: EirPlace::Ident("state".to_string()),
                reset: None,
                next: EirExpr::Int(0),
                reads: Vec::new(),
                origin,
            },
        ],
    );
    let design = match validated_design(vec![module]) {
        Ok(design) => design,
        Err(error) => panic!("test EIR should assemble: {error}"),
    };

    let facts = match DriverFactsCollector::new(&design).collect() {
        Ok(facts) => facts,
        Err(errors) => panic!("facts pass should succeed: {errors:?}"),
    };
    let drive = facts
        .drives()
        .first()
        .expect("clocked storage must produce a drive fact");

    assert_eq!(drive.target_place().display(), "state.next");
    assert!(matches!(
        drive.effect(),
        DriveEffect::Next { storage_target }
            if storage_target.display() == "state"
    ));
}
