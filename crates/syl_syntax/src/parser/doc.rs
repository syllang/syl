use super::Parser;
use crate::lexer::{Lexeme, LexemeKind, Token, TokenKind};
use crate::{Item, LosslessToken, LosslessTokenKind};
use std::collections::HashMap;
use syl_span::{Diagnostic, Span};

pub(super) struct PreparedLexemes {
    pub(super) tokens: Vec<Token>,
    pub(super) syntax_tokens: Vec<LosslessToken>,
    pub(super) doc_comments: HashMap<usize, CollectedDoc>,
    pub(super) module_doc: Option<String>,
    pub(super) diagnostics: Vec<Diagnostic>,
}

#[derive(Clone, Debug)]
pub(super) struct CollectedDoc {
    pub(super) text: String,
    pub(super) span: Span,
}

pub(super) fn prepare_lexemes(lexemes: Vec<Lexeme>) -> PreparedLexemes {
    let mut tokens = Vec::new();
    let mut syntax_tokens = Vec::new();
    let mut doc_comments = HashMap::new();
    let mut module_doc = None;
    let mut diagnostics = Vec::new();
    let mut pending_outer = PendingDoc::default();
    let mut pending_inner = PendingDoc::default();
    let mut seen_token = false;

    for lexeme in lexemes {
        let syntax_kind = syntax_kind_for_lexeme(&lexeme.kind);
        syntax_tokens.push(LosslessToken::new(
            syntax_kind,
            lexeme.span,
            lexeme.text.clone(),
        ));

        match lexeme.kind {
            LexemeKind::Token(kind) => {
                if let Some(doc) = pending_outer.take() {
                    doc_comments.insert(lexeme.span.start, doc);
                }
                if !seen_token && let Some(doc) = pending_inner.take() {
                    merge_doc(&mut module_doc, Some(doc.text));
                }
                seen_token = true;
                tokens.push(Token::new(kind, lexeme.span));
            }
            LexemeKind::Whitespace => {
                if has_blank_line(&lexeme.text) {
                    diagnose_unattached_doc(&mut diagnostics, &mut pending_outer);
                    if let Some(doc) = pending_inner.take() {
                        merge_doc(&mut module_doc, Some(doc.text));
                    }
                }
            }
            LexemeKind::LineComment => {
                if pending_outer.is_active() {
                    pending_outer.push_line(strip_line_comment(&lexeme.text), lexeme.span);
                } else if pending_inner.is_active() && !seen_token {
                    pending_inner.push_line(strip_line_comment(&lexeme.text), lexeme.span);
                }
            }
            LexemeKind::DocComment => {
                pending_outer.push_line(strip_doc_comment(&lexeme.text), lexeme.span);
            }
            LexemeKind::InnerDocComment => {
                if seen_token {
                    diagnostics.push(doc_diagnostic(
                        lexeme.span,
                        "`//!` doc comments are only valid before the first declaration",
                    ));
                } else {
                    pending_inner.push_line(strip_inner_doc_comment(&lexeme.text), lexeme.span);
                }
            }
            LexemeKind::Unknown => {
                diagnose_unattached_doc(&mut diagnostics, &mut pending_outer);
                if let Some(doc) = pending_inner.take() {
                    merge_doc(&mut module_doc, Some(doc.text));
                }
            }
        }
    }

    diagnose_unattached_doc(&mut diagnostics, &mut pending_outer);
    if let Some(doc) = pending_inner.take() {
        merge_doc(&mut module_doc, Some(doc.text));
    }

    PreparedLexemes {
        tokens,
        syntax_tokens,
        doc_comments,
        module_doc,
        diagnostics,
    }
}

impl Parser {
    pub(super) fn apply_item_doc(&mut self, item: &mut Item, doc: Option<String>) {
        let Some(doc) = doc else {
            return;
        };
        match item {
            Item::Error(_) => {}
            Item::Use(item) => item.doc = Some(doc),
            Item::Const(item) => item.doc = Some(doc),
            Item::Fn(item) => item.doc = Some(doc),
            Item::Enum(item) => item.doc = Some(doc),
            Item::Bundle(item) => item.doc = Some(doc),
            Item::Interface(item) => item.doc = Some(doc),
            Item::Map(item) => item.doc = Some(doc),
            Item::Cell(item) => item.doc = Some(doc),
            Item::ExternCell(item) => item.doc = Some(doc),
        }
    }

    pub(super) fn take_doc_for_next_token(&mut self) -> Option<String> {
        let start = self.peek()?.span.start;
        self.doc_comments.remove(&start).map(|doc| doc.text)
    }

    pub(super) fn merge_doc(
        &mut self,
        existing: Option<String>,
        incoming: Option<String>,
    ) -> Option<String> {
        match (existing, incoming) {
            (Some(mut existing), Some(incoming)) => {
                if !existing.is_empty() {
                    existing.push('\n');
                }
                existing.push_str(&incoming);
                Some(existing)
            }
            (Some(existing), None) => Some(existing),
            (None, Some(incoming)) => Some(incoming),
            (None, None) => None,
        }
    }
}

fn syntax_kind_for_lexeme(kind: &LexemeKind) -> LosslessTokenKind {
    match kind {
        LexemeKind::Token(kind) => match kind {
            TokenKind::Ident(_) => LosslessTokenKind::Ident,
            TokenKind::Int(_) => LosslessTokenKind::Int,
            TokenKind::Str(_) => LosslessTokenKind::Str,
            TokenKind::Bool(_) => LosslessTokenKind::Bool,
            TokenKind::KwUse
            | TokenKind::KwConst
            | TokenKind::KwFn
            | TokenKind::KwLet
            | TokenKind::KwReturn
            | TokenKind::KwThis
            | TokenKind::KwVar
            | TokenKind::KwFor
            | TokenKind::KwWhile
            | TokenKind::KwIf
            | TokenKind::KwElse
            | TokenKind::KwMatch
            | TokenKind::KwSelect
            | TokenKind::KwPriority
            | TokenKind::KwUnique
            | TokenKind::KwEnum
            | TokenKind::KwBundle
            | TokenKind::KwInterface
            | TokenKind::KwView
            | TokenKind::KwMap
            | TokenKind::KwCell
            | TokenKind::KwExtern
            | TokenKind::KwSignal
            | TokenKind::KwReg
            | TokenKind::KwPlace
            | TokenKind::KwInplace
            | TokenKind::KwNext
            | TokenKind::KwIn
            | TokenKind::KwInOut
            | TokenKind::KwOut
            | TokenKind::KwAnd
            | TokenKind::KwOr
            | TokenKind::KwNot
            | TokenKind::KwXor
            | TokenKind::KwEqWord => LosslessTokenKind::Keyword,
            TokenKind::At
            | TokenKind::Plus
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
            | TokenKind::Dot
            | TokenKind::DotDot
            | TokenKind::Comma
            | TokenKind::Colon
            | TokenKind::ColonEq
            | TokenKind::Semi
            | TokenKind::Arrow
            | TokenKind::EqGt
            | TokenKind::LParen
            | TokenKind::RParen
            | TokenKind::LBrace
            | TokenKind::RBrace
            | TokenKind::LBracket
            | TokenKind::RBracket => LosslessTokenKind::Punctuation,
        },
        LexemeKind::Whitespace => LosslessTokenKind::Whitespace,
        LexemeKind::LineComment => LosslessTokenKind::LineComment,
        LexemeKind::DocComment => LosslessTokenKind::DocComment,
        LexemeKind::InnerDocComment => LosslessTokenKind::InnerDocComment,
        LexemeKind::Unknown => LosslessTokenKind::Unknown,
    }
}

fn strip_doc_comment(text: &str) -> String {
    strip_comment_prefix(text, "///")
}

fn strip_inner_doc_comment(text: &str) -> String {
    strip_comment_prefix(text, "//!")
}

fn strip_line_comment(text: &str) -> String {
    strip_comment_prefix(text, "//")
}

fn strip_comment_prefix(text: &str, prefix: &str) -> String {
    let stripped = text.strip_prefix(prefix).unwrap_or(text);
    stripped.strip_prefix(' ').unwrap_or(stripped).to_string()
}

fn has_blank_line(text: &str) -> bool {
    text.chars().filter(|ch| *ch == '\n').count() > 1
}

fn merge_doc(target: &mut Option<String>, incoming: Option<String>) {
    let Some(incoming) = incoming else {
        return;
    };
    match target {
        Some(existing) if !existing.is_empty() => {
            existing.push('\n');
            existing.push_str(&incoming);
        }
        Some(existing) => existing.push_str(&incoming),
        None => *target = Some(incoming),
    }
}

fn diagnose_unattached_doc(diagnostics: &mut Vec<Diagnostic>, pending: &mut PendingDoc) {
    if let Some(span) = pending.take_span() {
        diagnostics.push(doc_diagnostic(
            span,
            "`///` doc comment must attach to a following declaration",
        ));
    }
}

fn doc_diagnostic(span: Span, message: impl Into<String>) -> Diagnostic {
    Diagnostic::new(span, message)
        .with_code("E_SYNTAX_DOC_COMMENT")
        .with_source("syl_syntax::parser")
}

#[derive(Default)]
struct PendingDoc {
    lines: Vec<String>,
    span: Option<Span>,
}

impl PendingDoc {
    fn is_active(&self) -> bool {
        !self.lines.is_empty()
    }

    fn push_line(&mut self, line: String, span: Span) {
        self.lines.push(line);
        self.span = Some(
            self.span
                .map(|existing| existing.join(span))
                .unwrap_or(span),
        );
    }

    fn take(&mut self) -> Option<CollectedDoc> {
        if self.lines.is_empty() {
            return None;
        }
        let span = self.span.take().unwrap_or_default();
        Some(CollectedDoc {
            text: std::mem::take(&mut self.lines).join("\n"),
            span,
        })
    }

    fn take_span(&mut self) -> Option<Span> {
        self.lines.clear();
        self.span.take()
    }
}
