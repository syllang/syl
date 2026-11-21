use super::*;
use crate::{CapabilityError, CompileError, DriverError, HirError};
use syl_span::SourceId;

#[test]
fn compile_error_converts_to_located_core_diagnostic() {
    let source = SourceId::new(2);
    let primary = Span::new_in(source, 10, 20);
    let related = Span::new_in(source, 1, 5);
    let error = CompileError::driver_error_with_related(
        DriverError::DuplicateHardwareDriver {
            name: "ready".to_string(),
        },
        primary,
        [(related, "previous driver claim".to_string())],
    );

    let diagnostic = error.to_diagnostic();

    assert_eq!(diagnostic.span, primary);
    assert_eq!(
        diagnostic.code.as_deref(),
        Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
    );
    assert_eq!(diagnostic.source.as_deref(), Some("syl_sema::driver"));
    assert_eq!(diagnostic.related.len(), 1);
    assert_eq!(diagnostic.related[0].span, related);
}

#[test]
fn compile_error_uses_specific_lsp_codes() {
    let unresolved = CompileError::lowering_at(
        HirError::UnresolvedName {
            name: "missing".to_string(),
        },
        Span::new(1, 8),
    )
    .to_diagnostic();
    let not_drivable = CompileError::lowering_at(
        CapabilityError::NotDrivable {
            target: "x".to_string(),
        },
        Span::new(10, 11),
    )
    .to_diagnostic();

    assert_eq!(unresolved.code.as_deref(), Some("E_MIDDLE_UNRESOLVED_NAME"));
    assert_eq!(not_drivable.code.as_deref(), Some("E_MIDDLE_NOT_DRIVABLE"));
}

#[test]
fn semantic_diagnostic_severity_reaches_core_diagnostic() {
    let semantic = SemanticDiagnostic::new(
        SemanticDiagnosticStage::Lowering,
        Span::new(2, 4),
        "W_MIDDLE",
        "warning",
    )
    .with_severity(DiagnosticSeverity::Warning);

    let diagnostic = Diagnostic::from(semantic);

    assert_eq!(diagnostic.severity, DiagnosticSeverity::Warning);
}
