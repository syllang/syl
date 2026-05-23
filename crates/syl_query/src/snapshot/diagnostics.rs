use crate::{
    DiagnosticRelatedResult, DiagnosticResult, DocumentDiagnostics,
    navigation::DiagnosticResultInput,
};
use syl_session::{AnalysisFile, AnalysisSnapshot, DocumentOrigin, DocumentUri, DocumentVersion};
use syl_span::{Diagnostic, DiagnosticRelatedInfo};

#[non_exhaustive]
pub(super) struct DiagnosticQueryEngine<'a> {
    snapshot: &'a AnalysisSnapshot,
}

impl<'a> DiagnosticQueryEngine<'a> {
    pub(super) fn new(snapshot: &'a AnalysisSnapshot) -> Self {
        Self { snapshot }
    }

    pub(super) fn all_document_diagnostics(&self) -> Vec<DocumentDiagnostics> {
        let diagnostics = self.all_core_diagnostics();
        self.snapshot
            .files()
            .iter()
            .map(|file| self.document_diagnostics_for_file(file, &diagnostics))
            .collect()
    }

    pub(super) fn document_diagnostics(&self, uri: &DocumentUri) -> Option<DocumentDiagnostics> {
        let file = self.snapshot.file_by_uri(uri)?;
        let diagnostics = self.all_core_diagnostics();
        Some(self.document_diagnostics_for_file(file, &diagnostics))
    }

    pub(super) fn diagnostics_for(&self, uri: &DocumentUri) -> Vec<DiagnosticResult> {
        self.document_diagnostics(uri)
            .map(|document| document.diagnostics().to_vec())
            .unwrap_or_default()
    }

    fn all_core_diagnostics(&self) -> Vec<Diagnostic> {
        let mut diagnostics = self.snapshot.diagnostics().to_vec();
        diagnostics.extend(self.snapshot.semantic_diagnostics());
        diagnostics
    }

    fn document_diagnostics_for_file(
        &self,
        file: &AnalysisFile,
        diagnostics: &[Diagnostic],
    ) -> DocumentDiagnostics {
        let diagnostics = diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.span.source == file.source_id())
            .filter_map(|diagnostic| self.diagnostic_result(diagnostic))
            .collect();
        DocumentDiagnostics::new(
            file.uri().clone(),
            Self::diagnostic_version(file),
            diagnostics,
        )
    }

    fn diagnostic_result(&self, diagnostic: &Diagnostic) -> Option<DiagnosticResult> {
        let range = self.snapshot.source_map().utf16_range(diagnostic.span)?;
        Some(DiagnosticResult::new(DiagnosticResultInput {
            range,
            severity: diagnostic.severity,
            code: diagnostic.code.clone(),
            source: diagnostic.source.clone(),
            message: diagnostic.message.clone(),
            related: self.related_diagnostics(&diagnostic.related),
        }))
    }

    fn related_diagnostics(
        &self,
        related: &[DiagnosticRelatedInfo],
    ) -> Vec<DiagnosticRelatedResult> {
        related
            .iter()
            .filter_map(|item| {
                let file = self.snapshot.source_map().file(item.span.source)?;
                let range = self.snapshot.source_map().utf16_range(item.span)?;
                Some(DiagnosticRelatedResult::new(
                    DocumentUri::new(file.uri()),
                    range,
                    item.message.clone(),
                ))
            })
            .collect()
    }

    fn diagnostic_version(file: &AnalysisFile) -> Option<DocumentVersion> {
        if matches!(file.origin(), DocumentOrigin::Overlay) {
            Some(file.version())
        } else {
            None
        }
    }
}
