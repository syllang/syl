use super::*;
use crate::{
    LoweringError,
    eir::{
        EirBinaryOp, EirExpansion, EirExpr, EirGuard, EirOrigin, EirPlace, EirSelectArm,
        EirSelectMode, EirUnaryOp,
    },
    eir::{
        EirDesign, EirDesignComposer, EirFactCollector, EirItem, EirModule, EirRawDesign,
        EirSignalActivity, EirValidator,
    },
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

fn origin() -> EirOrigin {
    EirOrigin::new(Span::new_in(SourceId::new(0), 0, 1), Vec::new())
}

fn tri_drive(target: &str, enable: EirExpr, value: u64) -> EirItem {
    EirItem::Drive {
        lhs: EirPlace::Ident(target.to_string()),
        rhs: EirExpr::select(
            EirSelectMode::Priority,
            vec![EirSelectArm::new(enable, EirExpr::Int(value))],
            EirExpr::HighZ,
        ),
        reads: Vec::new(),
        origin: origin(),
    }
}

fn optional_signal(name: &str) -> EirItem {
    EirItem::Signal {
        width: "1".into(),
        name: name.to_string(),
        activity: EirSignalActivity::Optional,
        origin: origin(),
    }
}

#[test]
fn tristate_drivers_with_opposite_enables_do_not_conflict() {
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![
            optional_signal("bus"),
            tri_drive("bus", EirExpr::ident("en"), 1),
            tri_drive(
                "bus",
                EirExpr::unary(EirUnaryOp::Not, EirExpr::ident("en")),
                0,
            ),
        ],
    );
    let design = validated_design(vec![module]).expect("test EIR should assemble");
    let facts = crate::driver::DriverFactsCollector::new(&design)
        .collect()
        .expect("facts pass should succeed");

    DriverDrcChecker::new(&design, &facts)
        .check_collect()
        .expect("opposite tri-state enables must not conflict");
}

#[test]
fn tristate_drivers_with_overlapping_enables_conflict() {
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![
            optional_signal("bus"),
            tri_drive("bus", EirExpr::ident("en"), 1),
            tri_drive(
                "bus",
                EirExpr::binary(
                    EirBinaryOp::AndAnd,
                    EirExpr::ident("en"),
                    EirExpr::ident("grant"),
                ),
                0,
            ),
        ],
    );
    let design = validated_design(vec![module]).expect("test EIR should assemble");
    let facts = crate::driver::DriverFactsCollector::new(&design)
        .collect()
        .expect("facts pass should succeed");
    let errors = match DriverDrcChecker::new(&design, &facts).check_collect() {
        Ok(_) => panic!("overlapping tri-state enables must conflict"),
        Err(errors) => errors,
    };

    assert!(errors.iter().any(|error| matches!(
        error,
        CompileError::Lowering { kind, .. }
            if matches!(
                kind.as_ref(),
                LoweringError::Driver(DriverError::DuplicateHardwareDriver { name })
                    if name == "bus"
            )
    )));
}

#[test]
fn duplicate_driver_keeps_expansion_call_stack_in_related_spans() {
    let source = SourceId::new(7);
    let call_span = Span::new_in(source, 40, 52);
    let previous_origin = EirOrigin::new(
        Span::new_in(source, 10, 16),
        vec![EirExpansion::new("DoubleDrive", "v", call_span)],
    );
    let conflicting_origin = EirOrigin::new(
        Span::new_in(source, 20, 26),
        vec![EirExpansion::new("DoubleDrive", "v", call_span)],
    );
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![
            EirItem::Signal {
                width: "1".into(),
                name: "v".to_string(),
                activity: crate::eir::EirSignalActivity::Required,
                origin: previous_origin.clone(),
            },
            EirItem::Drive {
                lhs: EirPlace::Ident("v".to_string()),
                rhs: EirExpr::Int(0),
                reads: Vec::new(),
                origin: previous_origin,
            },
            EirItem::Drive {
                lhs: EirPlace::Ident("v".to_string()),
                rhs: EirExpr::Int(1),
                reads: Vec::new(),
                origin: conflicting_origin,
            },
        ],
    );
    let design = match validated_design(vec![module]) {
        Ok(design) => design,
        Err(error) => panic!("test EIR should assemble: {error}"),
    };
    let facts = match crate::driver::DriverFactsCollector::new(&design).collect() {
        Ok(facts) => facts,
        Err(errors) => panic!("facts pass should succeed: {errors:?}"),
    };

    let errors = match DriverDrcChecker::new(&design, &facts).check_collect() {
        Ok(_) => panic!("duplicate driver must fail DRC"),
        Err(errors) => errors,
    };
    let diagnostic = errors
        .iter()
        .find_map(|error| match error {
            CompileError::Lowering { kind, diagnostic } => matches!(
                kind.as_ref(),
                LoweringError::Driver(DriverError::DuplicateHardwareDriver { name })
                    if name == "v"
            )
            .then_some(diagnostic.as_ref()),
            _ => None,
        })
        .expect("duplicate hardware driver diagnostic must be present");

    assert!(
        diagnostic
            .related
            .iter()
            .any(|related| related.span == call_span),
        "driver conflict diagnostics must include expansion callsite"
    );
    assert!(matches!(
        facts.drives().first().map(DriveFact::guard),
        Some(guard) if *guard == EirGuard::root()
    ));
}

#[test]
fn continuous_drive_to_storage_is_rejected_defensively() {
    let module = EirModule::new(
        "Top",
        Vec::new(),
        Vec::new(),
        vec![
            EirItem::Storage {
                width: "1".into(),
                name: "state".to_string(),
                origin: origin(),
            },
            EirItem::Drive {
                lhs: EirPlace::Ident("state".to_string()),
                rhs: EirExpr::Int(0),
                reads: Vec::new(),
                origin: origin(),
            },
        ],
    );
    let design = validated_design(vec![module]).expect("test EIR should assemble");
    let facts = crate::driver::DriverFactsCollector::new(&design)
        .collect()
        .expect("facts pass should succeed");
    let errors = match DriverDrcChecker::new(&design, &facts).check_collect() {
        Ok(_) => panic!("continuous drive to storage must fail DRC"),
        Err(errors) => errors,
    };

    assert!(errors.iter().any(|error| matches!(
        error,
        CompileError::Lowering { kind, .. }
            if matches!(
                kind.as_ref(),
                LoweringError::Driver(DriverError::ContinuousDriveTargetIsReg { name })
                    if name == "state"
            )
    )));
}
