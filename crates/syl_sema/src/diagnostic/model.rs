use strum_macros::IntoStaticStr;
use syl_span::{Diagnostic, DiagnosticRelatedInfo, DiagnosticSeverity, Span};

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum SemanticDiagnosticStage {
    #[strum(serialize = "lowering")]
    Lowering,
    #[strum(serialize = "driver")]
    Driver,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SemanticDiagnostic {
    span: Span,
    stage: SemanticDiagnosticStage,
    severity: DiagnosticSeverity,
    source: &'static str,
    code: &'static str,
    message: String,
    related: Vec<DiagnosticRelatedInfo>,
}

impl SemanticDiagnostic {
    pub fn new(
        stage: SemanticDiagnosticStage,
        span: Span,
        code: &'static str,
        message: impl Into<String>,
    ) -> Self {
        Self {
            span,
            stage,
            severity: DiagnosticSeverity::Error,
            source: "syl_sema",
            code,
            message: message.into(),
            related: Vec::new(),
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn stage(&self) -> SemanticDiagnosticStage {
        self.stage
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    pub fn source(&self) -> &'static str {
        self.source
    }

    pub fn code(&self) -> &'static str {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn related(&self) -> &[DiagnosticRelatedInfo] {
        &self.related
    }

    pub fn with_related(mut self, span: Span, message: impl Into<String>) -> Self {
        self.related.push(DiagnosticRelatedInfo::new(span, message));
        self
    }

    pub fn with_severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = severity;
        self
    }
}

impl From<&SemanticDiagnostic> for Diagnostic {
    fn from(value: &SemanticDiagnostic) -> Self {
        let stage: &'static str = value.stage.into();
        let mut diagnostic = Diagnostic::new(value.span, value.message.clone())
            .with_severity(value.severity)
            .with_code(value.code)
            .with_source(format!("{}::{stage}", value.source));
        for related in &value.related {
            diagnostic = diagnostic.with_related(related.clone());
        }
        diagnostic
    }
}

impl From<SemanticDiagnostic> for Diagnostic {
    fn from(value: SemanticDiagnostic) -> Self {
        Diagnostic::from(&value)
    }
}

#[cfg(test)]
mod tests {
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
}
