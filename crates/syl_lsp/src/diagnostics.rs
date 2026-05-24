use crate::mapping::LspMapper;
use std::collections::{BTreeMap, BTreeSet};
use syl_query::{
    DiagnosticRelatedResult, DiagnosticResult, DocumentDiagnostics, GroupedDiagnostics,
};
use syl_session::DocumentVersion;
use syl_session::ProjectError;
use syl_span::DiagnosticSeverity;
use tower_lsp::lsp_types::{
    Diagnostic as LspDiagnostic, DiagnosticRelatedInformation, DiagnosticSeverity as LspSeverity,
    Location, NumberOrString, Position, Range, Url,
};

#[derive(Clone, Debug)]
#[non_exhaustive]
pub(crate) struct LspDiagnosticPublication {
    uri: Url,
    version: Option<i32>,
    diagnostics: Vec<LspDiagnostic>,
}

impl LspDiagnosticPublication {
    pub(crate) fn new(uri: Url, version: Option<i32>, diagnostics: Vec<LspDiagnostic>) -> Self {
        Self {
            uri,
            version,
            diagnostics,
        }
    }

    pub(crate) fn project_error(uri: Url, error: ProjectError) -> Self {
        Self::new(
            uri,
            None,
            vec![LspDiagnostic {
                range: ProjectErrorRange::new().into_lsp(),
                severity: Some(LspSeverity::ERROR),
                code: Some(NumberOrString::String("E_PROJECT".to_string())),
                code_description: None,
                source: Some("syl_session".to_string()),
                message: error.to_string(),
                related_information: None,
                tags: None,
                data: None,
            }],
        )
    }

    pub(crate) fn into_parts(self) -> (Url, Vec<LspDiagnostic>, Option<i32>) {
        (self.uri, self.diagnostics, self.version)
    }

    fn uri_key(&self) -> String {
        self.uri.to_string()
    }

    #[cfg(test)]
    fn uri(&self) -> &Url {
        &self.uri
    }

    #[cfg(test)]
    fn version(&self) -> Option<i32> {
        self.version
    }
}

#[derive(Debug)]
#[non_exhaustive]
struct ProjectErrorRange {
    line: u32,
    character: u32,
}

impl ProjectErrorRange {
    fn new() -> Self {
        Self {
            line: 0,
            character: 0,
        }
    }

    fn into_lsp(self) -> Range {
        let position = Position::new(self.line, self.character);
        Range::new(position, position)
    }
}

#[derive(Debug, Default)]
#[non_exhaustive]
pub(crate) struct LspDiagnosticState {
    published: BTreeMap<String, Url>,
    project_errors: BTreeMap<String, Url>,
}

impl LspDiagnosticState {
    pub(crate) fn new() -> Self {
        Self {
            published: BTreeMap::new(),
            project_errors: BTreeMap::new(),
        }
    }

    pub(crate) fn record_project_error(
        &mut self,
        publication: LspDiagnosticPublication,
    ) -> Vec<LspDiagnosticPublication> {
        self.project_errors
            .insert(publication.uri_key(), publication.uri.clone());
        vec![publication]
    }

    pub(crate) fn reconcile(
        &mut self,
        mut current: Vec<LspDiagnosticPublication>,
    ) -> Vec<LspDiagnosticPublication> {
        let current_keys = current
            .iter()
            .map(LspDiagnosticPublication::uri_key)
            .collect::<BTreeSet<_>>();
        for (key, uri) in &self.published {
            if !current_keys.contains(key) {
                current.push(LspDiagnosticPublication::new(uri.clone(), None, Vec::new()));
            }
        }
        for (key, uri) in &self.project_errors {
            if !current_keys.contains(key) {
                current.push(LspDiagnosticPublication::new(uri.clone(), None, Vec::new()));
            }
        }
        self.published = current
            .iter()
            .map(|publication| (publication.uri_key(), publication.uri.clone()))
            .collect();
        self.project_errors.clear();
        current
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct LspDiagnostics<'a> {
    grouped: &'a GroupedDiagnostics,
    mapper: LspMapper,
    severity: LspSeverityMapper,
}

impl<'a> LspDiagnostics<'a> {
    pub(crate) fn new(grouped: &'a GroupedDiagnostics) -> Self {
        Self {
            grouped,
            mapper: LspMapper::new(),
            severity: LspSeverityMapper::new(),
        }
    }

    pub(crate) fn publications(&self) -> Vec<LspDiagnosticPublication> {
        self.grouped
            .packages()
            .into_iter()
            .flat_map(|package| package.documents().iter())
            .filter_map(|document| self.document_publication(document))
            .collect()
    }

    fn document_publication(
        &self,
        document: &DocumentDiagnostics,
    ) -> Option<LspDiagnosticPublication> {
        let uri = Url::parse(document.uri().as_str()).ok()?;
        let diagnostics = self.lsp_diagnostics(document.diagnostics());
        Some(LspDiagnosticPublication::new(
            uri,
            PublishVersion::new(document.version()).into_lsp(),
            diagnostics,
        ))
    }

    fn lsp_diagnostics(&self, diagnostics: &[DiagnosticResult]) -> Vec<LspDiagnostic> {
        diagnostics
            .iter()
            .map(|diagnostic| self.lsp_diagnostic(diagnostic))
            .collect()
    }

    fn lsp_diagnostic(&self, diagnostic: &DiagnosticResult) -> LspDiagnostic {
        LspDiagnostic {
            range: self.mapper.range(diagnostic.range()),
            severity: Some(self.severity.map(diagnostic.severity())),
            code: diagnostic
                .code()
                .map(|code| NumberOrString::String(code.to_string())),
            code_description: None,
            source: diagnostic.source().map(str::to_string),
            message: diagnostic.message().to_string(),
            related_information: self.related(diagnostic.related()),
            tags: None,
            data: None,
        }
    }

    fn related(
        &self,
        related: &[DiagnosticRelatedResult],
    ) -> Option<Vec<DiagnosticRelatedInformation>> {
        let mut out = Vec::new();
        for item in related {
            let Ok(uri) = Url::parse(item.uri().as_str()) else {
                continue;
            };
            out.push(DiagnosticRelatedInformation {
                location: Location::new(uri, self.mapper.range(item.range())),
                message: item.message().to_string(),
            });
        }
        if out.is_empty() { None } else { Some(out) }
    }
}

#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
struct LspSeverityMapper {
    fallback: LspSeverity,
}

impl LspSeverityMapper {
    fn new() -> Self {
        Self {
            fallback: LspSeverity::INFORMATION,
        }
    }

    fn map(&self, severity: DiagnosticSeverity) -> LspSeverity {
        match severity {
            DiagnosticSeverity::Error => LspSeverity::ERROR,
            DiagnosticSeverity::Warning => LspSeverity::WARNING,
            DiagnosticSeverity::Information => LspSeverity::INFORMATION,
            DiagnosticSeverity::Hint => LspSeverity::HINT,
            _ => self.fallback,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct PublishVersion {
    value: Option<i32>,
}

impl PublishVersion {
    fn into_lsp(self) -> Option<i32> {
        self.value
    }

    fn new(version: Option<DocumentVersion>) -> Self {
        let value = match version {
            Some(version) => match i32::try_from(version.get()) {
                Ok(value) => Some(value),
                Err(_) => Some(i32::MAX),
            },
            None => None,
        };
        Self { value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env, fs,
        time::{SystemTime, UNIX_EPOCH},
    };
    use syl_query::AnalysisQueries;
    use syl_session::{AnalysisHost, ProjectResolver};
    use syl_session::{DocumentUri, DocumentVersion};

    #[test]
    fn publications_carry_snapshot_file_versions() {
        let uri = DocumentUri::new("file:///tmp/syl_lsp_version.syl");
        let mut host = AnalysisHost::new();
        host.open_document(
            uri.clone(),
            "package app;\nmodule Broken(".to_string(),
            DocumentVersion::new(7),
        );
        let snapshot = host
            .snapshot()
            .expect("overlay-only snapshot should be available");

        let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
            .publications()
            .into_iter()
            .find(|publication| publication.uri().as_str() == uri.as_str())
            .expect("snapshot file must produce a diagnostic publication");

        assert_eq!(publication.version(), Some(7));
    }

    #[test]
    fn publications_preserve_facade_utf16_ranges() {
        let uri = DocumentUri::new("file:///tmp/syl_lsp_utf16.syl");
        let source = format!("package app;\nconst WIDTH: Nat = {}\n", '\u{1F4A1}');
        let mut host = AnalysisHost::new();
        host.open_document(uri.clone(), source, DocumentVersion::new(5));
        let snapshot = host
            .snapshot()
            .expect("UTF-16 diagnostic fixture should snapshot through parser recovery");
        let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
            .publications()
            .into_iter()
            .find(|publication| publication.uri().as_str() == uri.as_str())
            .expect("opened overlay must produce a diagnostic publication");
        let (_, diagnostics, version) = publication.into_parts();
        let lexer_diagnostic = diagnostics
            .iter()
            .find(|diagnostic| diagnostic.message.contains("unexpected character"))
            .expect("invalid non-ASCII token must publish a diagnostic");

        assert_eq!(version, Some(5));
        assert_eq!(lexer_diagnostic.range.start.line, 1);
        assert_eq!(
            lexer_diagnostic
                .range
                .end
                .character
                .saturating_sub(lexer_diagnostic.range.start.character),
            2
        );
    }

    #[test]
    fn diagnostic_state_clears_files_missing_from_next_snapshot() {
        let first_uri = Url::parse("file:///tmp/syl_lsp_removed.syl").expect("test URL must parse");
        let second_uri =
            Url::parse("file:///tmp/syl_lsp_current.syl").expect("test URL must parse");
        let mut state = LspDiagnosticState::new();

        let first = state.reconcile(vec![LspDiagnosticPublication::new(
            first_uri.clone(),
            Some(1),
            Vec::new(),
        )]);
        let second = state.reconcile(vec![LspDiagnosticPublication::new(
            second_uri.clone(),
            Some(2),
            Vec::new(),
        )]);

        assert_eq!(first.len(), 1);
        assert!(second.iter().any(|publication| {
            publication.uri() == &first_uri && publication.version().is_none()
        }));
        assert!(second.iter().any(|publication| {
            publication.uri() == &second_uri && publication.version() == Some(2)
        }));
    }

    #[test]
    fn disk_publications_do_not_carry_fake_versions() {
        let root = LspDiagnosticWorkspace::new();
        let path = root.write("broken.syl", "package app\nmodule Broken(");
        let snapshot = ProjectResolver::new()
            .load(&[path])
            .expect("diagnostic fixture must load through parser recovery")
            .snapshot()
            .clone();

        let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
            .publications()
            .into_iter()
            .find(|publication| publication.uri().as_str().ends_with("broken.syl"))
            .expect("disk file must produce a publication");

        assert_eq!(publication.version(), None);
    }

    #[test]
    fn publications_preserve_lsp_diagnostic_fields_and_related_locations() {
        let uri = DocumentUri::new("file:///tmp/syl_lsp_related.syl");
        let mut host = AnalysisHost::new();
        host.open_document(
            uri.clone(),
            "package app;\n\nmodule Bad(y: out Bit) {\n    y := 0\n    y := 1\n}\n".to_string(),
            DocumentVersion::new(11),
        );
        let snapshot = host
            .snapshot()
            .expect("related diagnostic snapshot should build");
        let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
            .publications()
            .into_iter()
            .find(|publication| publication.uri().as_str() == uri.as_str())
            .expect("overlay file must produce diagnostic publication");
        let (published_uri, diagnostics, version) = publication.into_parts();

        assert_eq!(published_uri.as_str(), uri.as_str());
        assert_eq!(version, Some(11));
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.code
                    == Some(NumberOrString::String(
                        "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER".to_string(),
                    ))
            })
            .expect("duplicate driver diagnostic must be published");
        assert_eq!(diagnostic.severity, Some(LspSeverity::ERROR));
        let expected_source = ["syl_sema", "::", "driver"].concat();
        assert_eq!(diagnostic.source.as_deref(), Some(expected_source.as_str()));
        let related = diagnostic
            .related_information
            .as_ref()
            .expect("duplicate driver diagnostic must include related claims");
        assert!(
            related
                .iter()
                .all(|item| item.location.uri == published_uri)
        );
        assert!(
            related
                .iter()
                .any(|item| item.message == "previous driver claim")
        );
        assert!(
            related
                .iter()
                .any(|item| item.message == "conflicting driver claim")
        );
    }

    #[test]
    fn publications_cover_parse_tir_and_query_stage_failures() {
        let parse_uri = DocumentUri::new("file:///tmp/syl_lsp_parse.syl");
        let tir_uri = DocumentUri::new("file:///tmp/syl_lsp_tir.syl");
        let driver_uri = DocumentUri::new("file:///tmp/syl_lsp_driver.syl");
        let mut host = AnalysisHost::new();
        host.open_document(
            parse_uri.clone(),
            "package parse;\nmodule Broken(".to_string(),
            DocumentVersion::new(1),
        );
        host.open_document(
            tir_uri.clone(),
            "package sema;\nmodule Bad(x: in Missing) {}\n".to_string(),
            DocumentVersion::new(1),
        );
        host.open_document(
            driver_uri.clone(),
            "package driver;\n\nmodule Bad(y: out Bit) {\n    y := 0\n    y := 1\n}\n".to_string(),
            DocumentVersion::new(1),
        );
        let snapshot = host
            .snapshot()
            .expect("partial diagnostic fixture must snapshot");
        let publications = LspDiagnostics::new(&snapshot.grouped_diagnostics()).publications();

        assert_publication_has_diagnostics(&publications, parse_uri.as_str());
        assert_publication_has_diagnostics(&publications, tir_uri.as_str());
        assert_publication_has_diagnostics(&publications, driver_uri.as_str());
    }

    #[non_exhaustive]
    struct LspDiagnosticWorkspace {
        root: std::path::PathBuf,
    }

    impl LspDiagnosticWorkspace {
        fn new() -> Self {
            let stamp = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .expect("system clock must be after Unix epoch for test path")
                .as_nanos();
            let root =
                env::temp_dir().join(format!("syl_lsp_diag_{}_{}", std::process::id(), stamp));
            fs::create_dir_all(&root).expect("diagnostic workspace must be creatable");
            Self { root }
        }

        fn write(&self, relative: &str, text: &str) -> std::path::PathBuf {
            let path = self.root.join(relative);
            fs::write(&path, text).expect("diagnostic fixture must be writable");
            path
        }
    }

    fn assert_publication_has_diagnostics(publications: &[LspDiagnosticPublication], uri: &str) {
        let publication = publications
            .iter()
            .find(|publication| publication.uri().as_str() == uri)
            .unwrap_or_else(|| panic!("missing LSP publication for {uri}"));
        let (_, diagnostics, _) = publication.clone().into_parts();
        assert!(
            !diagnostics.is_empty(),
            "expected partial diagnostics for {uri}"
        );
    }
}
