use crate::{
    DiagnosticPackage, DiagnosticRelatedResult, DiagnosticResult, DiagnosticStage,
    DocumentDiagnostics, GroupedDiagnostics, PackageDiagnostics, QueryError, StageDiagnostics,
    navigation::DiagnosticResultInput,
};
use std::collections::BTreeMap;
use syl_session::{
    AnalysisFile, AnalysisSnapshot, CancellationToken, DocumentOrigin, DocumentUri,
    DocumentVersion, PackageStageDiagnostics, ProjectError,
};
use syl_span::{Diagnostic, DiagnosticRelatedInfo};
use syl_syntax::Item;

#[non_exhaustive]
pub(super) struct DiagnosticQueryEngine<'a> {
    snapshot: &'a AnalysisSnapshot,
}

impl<'a> DiagnosticQueryEngine<'a> {
    pub(super) fn new(snapshot: &'a AnalysisSnapshot) -> Self {
        Self { snapshot }
    }

    pub(super) fn all_document_diagnostics(&self) -> Vec<DocumentDiagnostics> {
        self.grouped_diagnostics()
            .packages()
            .iter()
            .flat_map(|package| package.documents().iter().cloned())
            .collect()
    }

    pub(super) fn document_diagnostics(&self, uri: &DocumentUri) -> Option<DocumentDiagnostics> {
        self.grouped_diagnostics()
            .packages()
            .iter()
            .flat_map(PackageDiagnostics::documents)
            .find(|document| document.uri() == uri)
            .cloned()
    }

    pub(super) fn diagnostics_for(&self, uri: &DocumentUri) -> Vec<DiagnosticResult> {
        self.document_diagnostics(uri)
            .map(|document| document.diagnostics().to_vec())
            .unwrap_or_default()
    }

    pub(super) fn grouped_diagnostics(&self) -> GroupedDiagnostics {
        self.grouped_diagnostics_with_token(&CancellationToken::new())
            .unwrap_or_else(|_| GroupedDiagnostics::new(Vec::new()))
    }

    pub(super) fn grouped_diagnostics_with_token(
        &self,
        token: &CancellationToken,
    ) -> Result<GroupedDiagnostics, QueryError> {
        let mut packages = BTreeMap::<String, Vec<DocumentDiagnostics>>::new();
        let mut package_stage_sets = BTreeMap::<String, PackageStageDiagnostics>::new();
        for file in self.snapshot.files() {
            let package = self.package_for_file(file);
            if !package_stage_sets.contains_key(package.name()) {
                let stage_set = self
                    .snapshot
                    .package_stage_diagnostics_with_token(file.uri(), token)
                    .map_err(Self::map_error)?
                    .unwrap_or_default();
                package_stage_sets.insert(package.name().to_string(), stage_set);
            }
            let stage_set = package_stage_sets
                .get(package.name())
                .expect("package diagnostics should be cached for every visited file");
            let document = self.document_diagnostics_for_file(file, package.clone(), stage_set);
            packages
                .entry(package.name().to_string())
                .or_default()
                .push(document);
        }

        Ok(GroupedDiagnostics::new(
            packages
                .into_iter()
                .map(|(name, documents)| {
                    PackageDiagnostics::new(DiagnosticPackage::new(name), documents)
                })
                .collect(),
        ))
    }

    fn document_diagnostics_for_file(
        &self,
        file: &AnalysisFile,
        package: DiagnosticPackage,
        stage_sets: &PackageStageDiagnostics,
    ) -> DocumentDiagnostics {
        let stages = [
            (DiagnosticStage::Parse, stage_sets.parse()),
            (DiagnosticStage::Hir, stage_sets.hir()),
            (DiagnosticStage::Tir, stage_sets.tir()),
            (DiagnosticStage::Elaboration, stage_sets.elaboration()),
        ]
        .into_iter()
        .map(|(stage, diagnostics)| {
            StageDiagnostics::new(
                stage,
                diagnostics
                    .iter()
                    .filter(|diagnostic| diagnostic.span.source == file.source_id())
                    .filter_map(|diagnostic| self.diagnostic_result(diagnostic))
                    .collect(),
            )
        })
        .filter(|stage| !stage.diagnostics().is_empty())
        .collect();
        DocumentDiagnostics::new(
            package,
            file.uri().clone(),
            Self::diagnostic_version(file),
            stages,
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

    fn package_for_file(&self, file: &AnalysisFile) -> DiagnosticPackage {
        let name = file
            .ast()
            .items
            .iter()
            .find_map(|item| match item {
                Item::Package(item) if !item.path.is_empty() => Some(item.path.join(".")),
                _ => None,
            })
            .unwrap_or_else(|| file.uri().to_string());
        DiagnosticPackage::new(name)
    }

    fn diagnostic_version(file: &AnalysisFile) -> Option<DocumentVersion> {
        if matches!(file.origin(), DocumentOrigin::Overlay) {
            Some(file.version())
        } else {
            None
        }
    }

    fn map_error(error: ProjectError) -> QueryError {
        match error {
            ProjectError::Cancelled => QueryError::Cancelled,
            other => panic!("unexpected session error during snapshot query: {other}"),
        }
    }
}
