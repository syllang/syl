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
mod tests;
