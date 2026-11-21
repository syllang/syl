use crate::DocumentSymbolKind;
use std::collections::BTreeMap;
use syl_session::DocumentUri;
use syl_span::{SourcePosition, SourceRange};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LocationResult {
    pub uri: DocumentUri,
    pub range: SourceRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PrepareRenameResult {
    pub range: SourceRange,
    pub placeholder: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct TextEditResult {
    pub range: SourceRange,
    pub new_text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkspaceEditResult {
    pub changes: BTreeMap<DocumentUri, Vec<TextEditResult>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct WorkspaceSymbolResult {
    pub name: String,
    pub kind: DocumentSymbolKind,
    pub container_name: Option<String>,
    pub location: LocationResult,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum InlayHintKind {
    Type,
    Parameter,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct InlayHintResult {
    pub position: SourcePosition,
    pub label: String,
    pub kind: InlayHintKind,
    pub tooltip: Option<String>,
    pub padding_left: bool,
    pub padding_right: bool,
}

/// Canonical semantic token legend order (index == LSP legend index).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SemanticTokenKind {
    Namespace,
    Type,
    Enum,
    EnumMember,
    Struct,
    Function,
    Parameter,
    Variable,
    Property,
    TypeParameter,
    Keyword,
    Number,
    String,
    Operator,
    Comment,
    Decorator,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SemanticTokenModifier {
    Declaration,
    Definition,
    Readonly,
    Modification,
    Documentation,
    DefaultLibrary,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SemanticTokenResult {
    pub range: SourceRange,
    pub kind: SemanticTokenKind,
    pub modifiers: Vec<SemanticTokenModifier>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParameterInfoResult {
    pub label: String,
    pub documentation: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SignatureInfoResult {
    pub label: String,
    pub documentation: Option<String>,
    pub parameters: Vec<ParameterInfoResult>,
    pub active_parameter: Option<usize>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SignatureHelpResult {
    pub signatures: Vec<SignatureInfoResult>,
    pub active_signature: usize,
    pub active_parameter: Option<usize>,
}
