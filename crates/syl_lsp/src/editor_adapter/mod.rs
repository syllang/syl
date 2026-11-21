use crate::mapping::LspMapper;
use syl_query::{self as query, SemanticTokenKind as Kind, SemanticTokenModifier as Modifier};
use tower_lsp::{
    jsonrpc::{Error, ErrorCode},
    lsp_types as lsp,
};

pub(super) fn query_error(error: query::QueryError) -> Error {
    let code = match error {
        query::QueryError::Cancelled => ErrorCode::RequestCancelled,
        query::QueryError::InvalidRename { .. } => ErrorCode::InvalidParams,
        _ => ErrorCode::InternalError,
    };
    Error {
        code,
        message: error.to_string().into(),
        data: None,
    }
}

pub(super) fn location(result: query::LocationResult) -> Option<lsp::Location> {
    Some(lsp::Location::new(
        lsp::Url::parse(result.uri.as_str()).ok()?,
        LspMapper::new().range(result.range),
    ))
}

pub(super) fn prepare_rename(result: query::PrepareRenameResult) -> lsp::PrepareRenameResponse {
    lsp::PrepareRenameResponse::RangeWithPlaceholder {
        range: LspMapper::new().range(result.range),
        placeholder: result.placeholder,
    }
}

pub(super) fn workspace_edit(
    result: query::WorkspaceEditResult,
) -> Result<lsp::WorkspaceEdit, Error> {
    let mapper = LspMapper::new();
    let changes = result
        .changes
        .into_iter()
        .map(|(uri, edits)| {
            let uri = lsp::Url::parse(uri.as_str()).map_err(|_| Error::internal_error())?;
            let edits = edits
                .into_iter()
                .map(|edit| lsp::TextEdit {
                    range: mapper.range(edit.range),
                    new_text: edit.new_text,
                })
                .collect();
            Ok((uri, edits))
        })
        .collect::<Result<_, Error>>()?;
    Ok(lsp::WorkspaceEdit {
        changes: Some(changes),
        ..Default::default()
    })
}

#[allow(deprecated)]
pub(super) fn symbol(result: query::WorkspaceSymbolResult) -> Option<lsp::SymbolInformation> {
    use query::DocumentSymbolKind as K;
    let kind = match result.kind {
        K::Package => lsp::SymbolKind::PACKAGE,
        K::Module => lsp::SymbolKind::MODULE,
        K::Function => lsp::SymbolKind::FUNCTION,
        K::Type => lsp::SymbolKind::STRUCT,
        K::Constant => lsp::SymbolKind::CONSTANT,
        K::Field => lsp::SymbolKind::FIELD,
        K::Variable | K::Parameter => lsp::SymbolKind::VARIABLE,
        K::EnumMember => lsp::SymbolKind::ENUM_MEMBER,
        K::View => lsp::SymbolKind::NAMESPACE,
        _ => lsp::SymbolKind::OBJECT,
    };
    Some(lsp::SymbolInformation {
        name: result.name,
        kind,
        tags: None,
        deprecated: None,
        location: location(result.location)?,
        container_name: result.container_name,
    })
}

pub(super) fn inlay_hint(result: query::InlayHintResult) -> lsp::InlayHint {
    lsp::InlayHint {
        position: LspMapper::new().position(result.position),
        label: lsp::InlayHintLabel::String(result.label),
        kind: Some(match result.kind {
            query::InlayHintKind::Parameter => lsp::InlayHintKind::PARAMETER,
            _ => lsp::InlayHintKind::TYPE,
        }),
        text_edits: None,
        tooltip: result.tooltip.map(lsp::InlayHintTooltip::String),
        padding_left: Some(result.padding_left),
        padding_right: Some(result.padding_right),
        data: None,
    }
}

pub(super) fn signature_help(result: query::SignatureHelpResult) -> lsp::SignatureHelp {
    lsp::SignatureHelp {
        signatures: result
            .signatures
            .into_iter()
            .map(|s| lsp::SignatureInformation {
                label: s.label,
                documentation: s.documentation.map(lsp::Documentation::String),
                parameters: Some(
                    s.parameters
                        .into_iter()
                        .map(|p| lsp::ParameterInformation {
                            label: lsp::ParameterLabel::Simple(p.label),
                            documentation: p.documentation.map(lsp::Documentation::String),
                        })
                        .collect(),
                ),
                active_parameter: s.active_parameter.map(bounded),
            })
            .collect(),
        active_signature: Some(bounded(result.active_signature)),
        active_parameter: result.active_parameter.map(bounded),
    }
}

pub(super) fn semantic_tokens_legend() -> lsp::SemanticTokensLegend {
    use lsp::{SemanticTokenModifier as M, SemanticTokenType as T};
    lsp::SemanticTokensLegend {
        token_types: vec![
            T::NAMESPACE,
            T::TYPE,
            T::ENUM,
            T::ENUM_MEMBER,
            T::STRUCT,
            T::FUNCTION,
            T::PARAMETER,
            T::VARIABLE,
            T::PROPERTY,
            T::TYPE_PARAMETER,
            T::KEYWORD,
            T::NUMBER,
            T::STRING,
            T::OPERATOR,
            T::COMMENT,
            T::DECORATOR,
        ],
        token_modifiers: vec![
            M::DECLARATION,
            M::DEFINITION,
            M::READONLY,
            M::MODIFICATION,
            M::DOCUMENTATION,
            M::DEFAULT_LIBRARY,
        ],
    }
}

pub(super) fn encode_semantic_tokens(
    tokens: Vec<query::SemanticTokenResult>,
) -> lsp::SemanticTokensResult {
    let mapper = LspMapper::new();
    let mut previous = lsp::Position::default();
    let mut data = Vec::new();
    for token in tokens {
        let range = mapper.range(token.range);
        if range.start.line != range.end.line || range.start.character >= range.end.character {
            continue;
        }
        let Some(delta_line) = range.start.line.checked_sub(previous.line) else {
            continue;
        };
        let Some(delta_start) = (if delta_line == 0 {
            range.start.character.checked_sub(previous.character)
        } else {
            Some(range.start.character)
        }) else {
            continue;
        };
        data.push(lsp::SemanticToken {
            delta_line,
            delta_start,
            length: range.end.character - range.start.character,
            token_type: kind_index(token.kind),
            token_modifiers_bitset: token
                .modifiers
                .into_iter()
                .fold(0, |bits, m| bits | modifier_bit(m)),
        });
        previous = range.start;
    }
    lsp::SemanticTokensResult::Tokens(lsp::SemanticTokens {
        result_id: None,
        data,
    })
}

fn bounded(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn kind_index(kind: Kind) -> u32 {
    match kind {
        Kind::Namespace => 0,
        Kind::Type => 1,
        Kind::Enum => 2,
        Kind::EnumMember => 3,
        Kind::Struct => 4,
        Kind::Function => 5,
        Kind::Parameter => 6,
        Kind::Variable => 7,
        Kind::Property => 8,
        Kind::TypeParameter => 9,
        Kind::Keyword => 10,
        Kind::Number => 11,
        Kind::String => 12,
        Kind::Operator => 13,
        Kind::Comment => 14,
        Kind::Decorator => 15,
        _ => 7,
    }
}

fn modifier_bit(modifier: Modifier) -> u32 {
    match modifier {
        Modifier::Declaration => 1,
        Modifier::Definition => 2,
        Modifier::Readonly => 4,
        Modifier::Modification => 8,
        Modifier::Documentation => 16,
        Modifier::DefaultLibrary => 32,
        _ => 0,
    }
}

#[cfg(test)]
mod tests;
