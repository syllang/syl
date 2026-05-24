use syl_session::{DocumentUri, DocumentVersion};
use syl_span::{DiagnosticSeverity, SourceRange};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiagnosticPackage {
    name: String,
}

impl DiagnosticPackage {
    pub(crate) fn new(name: String) -> Self {
        Self { name }
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiagnosticRelatedResult {
    uri: DocumentUri,
    range: SourceRange,
    message: String,
}

impl DiagnosticRelatedResult {
    pub(crate) fn new(uri: DocumentUri, range: SourceRange, message: String) -> Self {
        Self {
            uri,
            range,
            message,
        }
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn range(&self) -> SourceRange {
        self.range
    }

    pub fn message(&self) -> &str {
        &self.message
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticStage {
    Parse,
    Hir,
    Tir,
    Elaboration,
    Backend,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiagnosticResult {
    range: SourceRange,
    severity: DiagnosticSeverity,
    code: Option<String>,
    source: Option<String>,
    message: String,
    related: Vec<DiagnosticRelatedResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) struct DiagnosticResultInput {
    pub(crate) range: SourceRange,
    pub(crate) severity: DiagnosticSeverity,
    pub(crate) code: Option<String>,
    pub(crate) source: Option<String>,
    pub(crate) message: String,
    pub(crate) related: Vec<DiagnosticRelatedResult>,
}

impl DiagnosticResult {
    pub(crate) fn new(input: DiagnosticResultInput) -> Self {
        let DiagnosticResultInput {
            range,
            severity,
            code,
            source,
            message,
            related,
        } = input;
        Self {
            range,
            severity,
            code,
            source,
            message,
            related,
        }
    }

    pub fn range(&self) -> SourceRange {
        self.range
    }

    pub fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    pub fn code(&self) -> Option<&str> {
        self.code.as_deref()
    }

    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn related(&self) -> &[DiagnosticRelatedResult] {
        &self.related
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DocumentDiagnostics {
    package: DiagnosticPackage,
    uri: DocumentUri,
    version: Option<DocumentVersion>,
    diagnostics: Vec<DiagnosticResult>,
    stages: Vec<StageDiagnostics>,
}

impl DocumentDiagnostics {
    pub(crate) fn new(
        package: DiagnosticPackage,
        uri: DocumentUri,
        version: Option<DocumentVersion>,
        stages: Vec<StageDiagnostics>,
    ) -> Self {
        let diagnostics = stages
            .iter()
            .flat_map(|stage| stage.diagnostics().iter().cloned())
            .collect();
        Self {
            package,
            uri,
            version,
            diagnostics,
            stages,
        }
    }

    pub fn package(&self) -> &DiagnosticPackage {
        &self.package
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn version(&self) -> Option<DocumentVersion> {
        self.version
    }

    pub fn diagnostics(&self) -> &[DiagnosticResult] {
        &self.diagnostics
    }

    pub fn stages(&self) -> &[StageDiagnostics] {
        &self.stages
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct StageDiagnostics {
    stage: DiagnosticStage,
    diagnostics: Vec<DiagnosticResult>,
}

impl StageDiagnostics {
    pub(crate) fn new(stage: DiagnosticStage, diagnostics: Vec<DiagnosticResult>) -> Self {
        Self { stage, diagnostics }
    }

    pub fn stage(&self) -> DiagnosticStage {
        self.stage
    }

    pub fn diagnostics(&self) -> &[DiagnosticResult] {
        &self.diagnostics
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageDiagnostics {
    package: DiagnosticPackage,
    documents: Vec<DocumentDiagnostics>,
}

impl PackageDiagnostics {
    pub(crate) fn new(package: DiagnosticPackage, documents: Vec<DocumentDiagnostics>) -> Self {
        Self { package, documents }
    }

    pub fn package(&self) -> &DiagnosticPackage {
        &self.package
    }

    pub fn documents(&self) -> &[DocumentDiagnostics] {
        &self.documents
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct GroupedDiagnostics {
    packages: Vec<PackageDiagnostics>,
}

impl GroupedDiagnostics {
    pub(crate) fn new(packages: Vec<PackageDiagnostics>) -> Self {
        Self { packages }
    }

    pub fn packages(&self) -> &[PackageDiagnostics] {
        &self.packages
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct HoverResult {
    pub contents: String,
    pub range: Option<SourceRange>,
}

impl HoverResult {
    pub fn new(contents: impl Into<String>, range: Option<SourceRange>) -> Self {
        Self {
            contents: contents.into(),
            range,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DefinitionResult {
    pub uri: DocumentUri,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentSymbolKind {
    Package,
    Module,
    Function,
    Type,
    Constant,
    Field,
    Variable,
    Parameter,
    EnumMember,
    View,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DocumentSymbolResult {
    pub name: String,
    pub kind: DocumentSymbolKind,
    pub range: SourceRange,
    pub selection_range: SourceRange,
    pub children: Vec<DocumentSymbolResult>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum CompletionItemKind {
    Module,
    Function,
    Type,
    Constant,
    Field,
    Keyword,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct CompletionResult {
    pub items: Vec<CompletionItem>,
}
