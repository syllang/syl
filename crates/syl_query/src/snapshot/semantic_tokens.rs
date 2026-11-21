use super::{
    editor_api::{check, map_error},
    ident_span, references,
    rename::builtin,
};
use crate::{
    QueryError, SemanticTokenKind as Kind, SemanticTokenModifier as Modifier, SemanticTokenResult,
};
use std::collections::BTreeMap;
use syl_hir::{HirDefKind, HirLocalKind};
use syl_sema::{HirAnalysis, SemanticResolution};
use syl_session::{AnalysisSnapshot, CancellationToken, DocumentUri};
use syl_span::Span;
use syl_syntax::{
    AstNodeKind,
    lexer::{LexemeKind, LosslessLexer},
    token::TokenKind,
};

type Class = (Kind, Vec<Modifier>);
type Classes = BTreeMap<(usize, usize), Class>;

pub(super) fn query(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    token: &CancellationToken,
) -> Result<Vec<SemanticTokenResult>, QueryError> {
    check(token)?;
    let Some(file) = snapshot.file_by_uri(uri) else {
        return Ok(Vec::new());
    };
    let Some(source) = snapshot.source_map().file(file.source_id()) else {
        return Ok(Vec::new());
    };
    let hir = snapshot
        .hir_analysis_for_uri_with_token(uri, token)
        .map_err(map_error)?;
    let mut classes = Classes::new();
    // Syntax classifications remain available for partially recovered declarations.
    for node in file.ast_node_index().nodes() {
        check(token)?;
        let kind = match node.kind() {
            AstNodeKind::CellItem | AstNodeKind::ExternCellItem => Kind::Struct,
            AstNodeKind::FnItem | AstNodeKind::MapItem => Kind::Function,
            AstNodeKind::EnumItem => Kind::Enum,
            AstNodeKind::BundleItem | AstNodeKind::StructItem | AstNodeKind::InterfaceItem => {
                Kind::Type
            }
            AstNodeKind::EnumVariant => Kind::EnumMember,
            AstNodeKind::FieldDecl | AstNodeKind::ViewDecl | AstNodeKind::ViewField => {
                Kind::Property
            }
            AstNodeKind::Attribute => Kind::Decorator,
            AstNodeKind::GenericParam => Kind::TypeParameter,
            AstNodeKind::Param | AstNodeKind::PortDecl | AstNodeKind::ResultBinding => {
                Kind::Parameter
            }
            _ => continue,
        };
        if let Some(ident) = ident_span::tokens(snapshot.source_map(), node.span())
            .into_iter()
            .find(|t| matches!(t.kind, TokenKind::Ident(_)))
        {
            classes.insert(
                (ident.span.start, ident.span.end),
                (
                    kind,
                    if kind == Kind::Decorator {
                        vec![]
                    } else {
                        vec![Modifier::Declaration]
                    },
                ),
            );
        }
    }
    if let Some(hir) = hir {
        for site in references::collect_sites(snapshot, hir, token)? {
            if site.span.source != file.source_id() {
                continue;
            }
            if let Some(mut class) = target_class(snapshot, hir, site.target) {
                if site.declaration {
                    class.1.push(Modifier::Declaration);
                    if matches!(site.target, SemanticResolution::Def(_)) {
                        class.1.push(Modifier::Definition);
                    }
                }
                classes.insert((site.span.start, site.span.end), class);
            }
        }
        for field in hir
            .design()
            .field_accesses()
            .iter()
            .filter(|f| f.span.source == file.source_id())
        {
            if let Some(span) =
                ident_span::name_span(snapshot.source_map(), field.span, &field.field, true)
            {
                let enum_base = hir
                    .resolution()
                    .get(syl_sema::HirFactId::Expr(field.base.id()))
                    .and_then(|r| match r {
                        SemanticResolution::Def(id) => hir.design().def(id),
                        _ => None,
                    })
                    .is_some_and(|d| d.kind == HirDefKind::Enum);
                classes.insert(
                    (span.start, span.end),
                    (
                        if enum_base {
                            Kind::EnumMember
                        } else {
                            Kind::Property
                        },
                        vec![],
                    ),
                );
            }
        }
    }
    for node in file
        .ast_node_index()
        .nodes()
        .iter()
        .filter(|n| n.kind() == AstNodeKind::UseItem)
    {
        for ident in ident_span::tokens(snapshot.source_map(), node.span())
            .into_iter()
            .filter(|t| matches!(t.kind, TokenKind::Ident(_)))
        {
            classes
                .entry((ident.span.start, ident.span.end))
                .or_insert((Kind::Namespace, vec![]));
        }
    }
    let mut result = Vec::new();
    for lexeme in LosslessLexer::new_in(source.text(), file.source_id())
        .lex_all_partial()
        .lexemes
    {
        check(token)?;
        let class = classes
            .get(&(lexeme.span.start, lexeme.span.end))
            .cloned()
            .or_else(|| lexical(&lexeme.kind));
        let Some((kind, modifiers)) = class else {
            continue;
        };
        let mut offset = lexeme.span.start;
        for line in lexeme.text.split_inclusive('\n') {
            let text = line.trim_end_matches(['\r', '\n']);
            if !text.is_empty()
                && let Some(range) =
                    source.utf16_range(Span::new_in(file.source_id(), offset, offset + text.len()))
            {
                result.push(SemanticTokenResult {
                    range,
                    kind,
                    modifiers: modifiers.clone(),
                });
            }
            offset += line.len();
        }
    }
    Ok(result)
}

fn target_class(
    snapshot: &AnalysisSnapshot,
    hir: &HirAnalysis,
    target: SemanticResolution,
) -> Option<Class> {
    let mut modifiers = Vec::new();
    let kind = match target {
        SemanticResolution::Def(id) => match hir.design().def(id)?.kind {
            HirDefKind::Cell | HirDefKind::ExternCell => Kind::Struct,
            HirDefKind::Fn | HirDefKind::Map => Kind::Function,
            HirDefKind::Enum => Kind::Enum,
            HirDefKind::Const => {
                modifiers.push(Modifier::Readonly);
                Kind::Variable
            }
            _ => Kind::Type,
        },
        SemanticResolution::Local(id) => {
            let local = hir.design().local(id)?;
            match local.kind {
                HirLocalKind::Generic => Kind::TypeParameter,
                HirLocalKind::Param | HirLocalKind::Result => {
                    let tokens = ident_span::tokens(snapshot.source_map(), local.span);
                    if tokens
                        .iter()
                        .any(|t| matches!(t.kind, TokenKind::KwOut | TokenKind::KwInOut))
                    {
                        modifiers.push(Modifier::Modification);
                    } else if tokens.iter().any(|t| t.kind == TokenKind::KwIn) {
                        modifiers.push(Modifier::Readonly);
                    }
                    Kind::Parameter
                }
                HirLocalKind::Const | HirLocalKind::Let => {
                    modifiers.push(Modifier::Readonly);
                    Kind::Variable
                }
                HirLocalKind::Var | HirLocalKind::Reg => {
                    modifiers.push(Modifier::Modification);
                    Kind::Variable
                }
                _ => Kind::Variable,
            }
        }
        _ => return None,
    };
    Some((kind, modifiers))
}

fn lexical(lexeme: &LexemeKind) -> Option<Class> {
    let mut modifiers = Vec::new();
    let kind = match lexeme {
        LexemeKind::DocComment | LexemeKind::InnerDocComment => {
            modifiers.push(Modifier::Documentation);
            Kind::Comment
        }
        LexemeKind::LineComment => Kind::Comment,
        LexemeKind::Token(kind) => match kind {
            TokenKind::Ident(name) if builtin(name) => {
                modifiers.push(Modifier::DefaultLibrary);
                Kind::Type
            }
            TokenKind::Ident(_) => Kind::Variable,
            TokenKind::Int(_) => Kind::Number,
            TokenKind::Str(_) => Kind::String,
            TokenKind::At => Kind::Decorator,
            TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::LBrace
            | TokenKind::RBrace
            | TokenKind::LBracket
            | TokenKind::RBracket
            | TokenKind::Comma
            | TokenKind::Semi
            | TokenKind::Colon
            | TokenKind::Dot => return None,
            TokenKind::Plus
            | TokenKind::Minus
            | TokenKind::Star
            | TokenKind::Slash
            | TokenKind::Percent
            | TokenKind::Eq
            | TokenKind::EqEq
            | TokenKind::Bang
            | TokenKind::BangEq
            | TokenKind::Lt
            | TokenKind::LtEq
            | TokenKind::LtLt
            | TokenKind::Gt
            | TokenKind::GtEq
            | TokenKind::AndAnd
            | TokenKind::OrOr
            | TokenKind::Arrow
            | TokenKind::EqGt
            | TokenKind::ColonEq
            | TokenKind::DotDot
            | TokenKind::KwAnd
            | TokenKind::KwOr
            | TokenKind::KwNot
            | TokenKind::KwXor
            | TokenKind::KwEqWord => Kind::Operator,
            _ => Kind::Keyword,
        },
        _ => return None,
    };
    Some((kind, modifiers))
}
