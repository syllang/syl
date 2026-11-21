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

#[derive(Clone, Debug, PartialEq)]
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
    latest_generation: u64,
    fast_by_uri: BTreeMap<String, LspDiagnosticPublication>,
    elab_by_uri: BTreeMap<String, LspDiagnosticPublication>,
    published: BTreeMap<String, Url>,
    project_errors: BTreeMap<String, Url>,
}

impl LspDiagnosticState {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn record_project_error(
        &mut self,
        publication: LspDiagnosticPublication,
    ) -> Vec<LspDiagnosticPublication> {
        self.elab_by_uri.remove(&publication.uri_key());
        self.fast_by_uri.remove(&publication.uri_key());
        self.project_errors
            .insert(publication.uri_key(), publication.uri.clone());
        vec![publication]
    }

    pub(crate) fn apply_fast_layer(
        &mut self,
        generation: u64,
        current: Vec<LspDiagnosticPublication>,
    ) -> Vec<LspDiagnosticPublication> {
        if generation < self.latest_generation {
            return Vec::new();
        }
        if generation > self.latest_generation {
            self.elab_by_uri.clear();
        }
        self.latest_generation = generation;
        self.fast_by_uri = current.into_iter().map(|p| (p.uri_key(), p)).collect();
        self.elab_by_uri
            .retain(|key, _| self.fast_by_uri.contains_key(key));
        let current = self
            .fast_by_uri
            .keys()
            .filter_map(|key| self.merged(key))
            .collect();
        self.reconcile(current)
    }

    pub(crate) fn apply_elaboration_layer(
        &mut self,
        generation: u64,
        current: Vec<LspDiagnosticPublication>,
    ) -> Vec<LspDiagnosticPublication> {
        if generation != self.latest_generation {
            return Vec::new();
        }
        let mut next = BTreeMap::new();
        for publication in current {
            let key = publication.uri_key();
            if let Some(fast) = self.fast_by_uri.get(&key)
                && fast
                    .version
                    .is_some_and(|v| publication.version.is_none_or(|w| w < v))
            {
                if let Some(old) = self.elab_by_uri.get(&key) {
                    next.insert(key, old.clone());
                }
                continue;
            }
            if !publication.diagnostics.is_empty() && !self.project_errors.contains_key(&key) {
                next.insert(key, publication);
            }
        }
        let keys = self
            .elab_by_uri
            .keys()
            .chain(next.keys())
            .cloned()
            .collect::<BTreeSet<_>>();
        let before = keys
            .iter()
            .map(|key| (key.clone(), self.merged(key)))
            .collect::<BTreeMap<_, _>>();
        self.elab_by_uri = next;
        let mut changed = Vec::new();
        for key in keys {
            let after = self.merged(&key);
            if before[&key] == after {
                continue;
            }
            let publication = after.unwrap_or_else(|| {
                LspDiagnosticPublication::new(
                    before[&key]
                        .as_ref()
                        .expect("previous publication")
                        .uri
                        .clone(),
                    None,
                    Vec::new(),
                )
            });
            self.published.insert(key, publication.uri.clone());
            changed.push(publication);
        }
        changed
    }

    fn merged(&self, key: &str) -> Option<LspDiagnosticPublication> {
        let mut publication = self
            .fast_by_uri
            .get(key)
            .or_else(|| self.elab_by_uri.get(key))?
            .clone();
        if self.fast_by_uri.contains_key(key)
            && let Some(elaboration) = self.elab_by_uri.get(key)
        {
            publication
                .diagnostics
                .extend(elaboration.diagnostics.clone());
        }
        Some(publication)
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
            .iter()
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
mod tests;
