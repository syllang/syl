use super::scanner::Scanner;
use crate::token::{Token, TokenKind};
use syl_span::{Diagnostic, SourceId, Span};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LexemeKind {
    /// Syntax-significant token returned by the semantic lexer.
    Token(TokenKind),
    /// Whitespace preserved for source reconstruction.
    Whitespace,
    /// Line comment preserved as trivia.
    LineComment,
    /// Outer documentation comment attached to the next declaration.
    DocComment,
    /// File-level documentation comment.
    InnerDocComment,
    /// Unrecognized source fragment.
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Lexeme {
    pub kind: LexemeKind,
    pub span: Span,
    pub text: Box<str>,
}

impl Lexeme {
    pub fn new(kind: LexemeKind, span: Span, text: impl Into<Box<str>>) -> Self {
        Self {
            kind,
            span,
            text: text.into(),
        }
    }

    pub fn into_token(self) -> Option<Token> {
        match self.kind {
            LexemeKind::Token(kind) => Some(Token::new(kind, self.span)),
            LexemeKind::Whitespace
            | LexemeKind::LineComment
            | LexemeKind::DocComment
            | LexemeKind::InnerDocComment
            | LexemeKind::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessLexOutput {
    pub lexemes: Vec<Lexeme>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LosslessLexOutput {
    pub fn new(lexemes: Vec<Lexeme>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            lexemes,
            diagnostics,
        }
    }
}

#[non_exhaustive]
pub struct LosslessLexer<'a> {
    scanner: Scanner<'a>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> LosslessLexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self {
            scanner: Scanner::new(source),
            diagnostics: Vec::new(),
        }
    }

    pub fn new_in(source: &'a str, source_id: SourceId) -> Self {
        Self {
            scanner: Scanner::new_in(source, source_id),
            diagnostics: Vec::new(),
        }
    }

    pub fn lex_all(&mut self) -> Result<Vec<Lexeme>, Vec<Diagnostic>> {
        let output = self.lex_all_partial();
        if output.diagnostics.is_empty() {
            Ok(output.lexemes)
        } else {
            Err(output.diagnostics)
        }
    }

    pub fn lex_all_partial(&mut self) -> LosslessLexOutput {
        let mut lexemes = Vec::new();
        while let Some((i, ch)) = self.scanner.peek() {
            match ch {
                c if c.is_whitespace() => lexemes.push(self.lex_whitespace()),
                '/' if self.scanner.peek_next('/') => lexemes.push(self.lex_line_comment()),
                '/' => {
                    let token = self.scanner.single(TokenKind::Slash);
                    lexemes.push(self.lossless_normal(token));
                }
                '0'..='9' => {
                    let token = self.scanner.lex_number();
                    lexemes.push(self.lossless_normal(token));
                }
                '"' => match self.scanner.lex_string() {
                    Ok(token) => lexemes.push(self.lossless_normal(token)),
                    Err(mut diagnostics) => {
                        let end = self
                            .scanner
                            .peek()
                            .map(|(index, _)| index)
                            .unwrap_or(self.scanner.source_len());
                        let span = self.scanner.span(i, end);
                        self.diagnostics.append(&mut diagnostics);
                        lexemes.push(self.lossless_unknown(span));
                    }
                },
                'a'..='z' | 'A'..='Z' | '_' => {
                    let token = self.scanner.lex_ident_or_keyword();
                    lexemes.push(self.lossless_normal(token));
                }
                '+' => {
                    let token = self.scanner.single(TokenKind::Plus);
                    lexemes.push(self.lossless_normal(token));
                }
                '-' => {
                    let token = self.lex_dash(i);
                    lexemes.push(self.lossless_normal(token));
                }
                '*' => {
                    let token = self.scanner.single(TokenKind::Star);
                    lexemes.push(self.lossless_normal(token));
                }
                '%' => {
                    let token = self.scanner.single(TokenKind::Percent);
                    lexemes.push(self.lossless_normal(token));
                }
                '=' => {
                    let token = self.lex_eq(i);
                    lexemes.push(self.lossless_normal(token));
                }
                '!' => {
                    let token = self.lex_bang(i);
                    lexemes.push(self.lossless_normal(token));
                }
                '<' => {
                    let token = self.lex_lt(i);
                    lexemes.push(self.lossless_normal(token));
                }
                '>' => {
                    let token = self.lex_gt(i);
                    lexemes.push(self.lossless_normal(token));
                }
                '&' => lexemes.push(self.lex_amp(i)),
                '|' => lexemes.push(self.lex_pipe(i)),
                '@' => {
                    let token = self.scanner.single(TokenKind::At);
                    lexemes.push(self.lossless_normal(token));
                }
                '.' => {
                    let token = self.lex_dot(i);
                    lexemes.push(self.lossless_normal(token));
                }
                ',' => {
                    let token = self.scanner.single(TokenKind::Comma);
                    lexemes.push(self.lossless_normal(token));
                }
                ':' => {
                    let token = self.lex_colon(i);
                    lexemes.push(self.lossless_normal(token));
                }
                ';' => {
                    let token = self.scanner.single(TokenKind::Semi);
                    lexemes.push(self.lossless_normal(token));
                }
                '(' => {
                    let token = self.scanner.single(TokenKind::LParen);
                    lexemes.push(self.lossless_normal(token));
                }
                ')' => {
                    let token = self.scanner.single(TokenKind::RParen);
                    lexemes.push(self.lossless_normal(token));
                }
                '{' => {
                    let token = self.scanner.single(TokenKind::LBrace);
                    lexemes.push(self.lossless_normal(token));
                }
                '}' => {
                    let token = self.scanner.single(TokenKind::RBrace);
                    lexemes.push(self.lossless_normal(token));
                }
                '[' => {
                    let token = self.scanner.single(TokenKind::LBracket);
                    lexemes.push(self.lossless_normal(token));
                }
                ']' => {
                    let token = self.scanner.single(TokenKind::RBracket);
                    lexemes.push(self.lossless_normal(token));
                }
                _ => {
                    self.scanner.next();
                    let span = self.scanner.span(i, i + ch.len_utf8());
                    self.diagnostics
                        .push(self.scanner.unexpected_character_diagnostic(
                            span,
                            format!("unexpected character {ch:?}"),
                        ));
                    lexemes.push(self.lossless_unknown(span));
                }
            }
        }
        LosslessLexOutput::new(lexemes, std::mem::take(&mut self.diagnostics))
    }

    fn lex_dash(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('>') {
            Token::new(TokenKind::Arrow, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Minus, self.scanner.span(start, start + 1))
        }
    }

    fn lex_eq(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('=') {
            Token::new(TokenKind::EqEq, self.scanner.span(start, start + 2))
        } else if self.scanner.match_char('>') {
            Token::new(TokenKind::EqGt, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Eq, self.scanner.span(start, start + 1))
        }
    }

    fn lex_bang(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('=') {
            Token::new(TokenKind::BangEq, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Bang, self.scanner.span(start, start + 1))
        }
    }

    fn lex_lt(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('=') {
            Token::new(TokenKind::LtEq, self.scanner.span(start, start + 2))
        } else if self.scanner.match_char('<') {
            Token::new(TokenKind::LtLt, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Lt, self.scanner.span(start, start + 1))
        }
    }

    fn lex_gt(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('=') {
            Token::new(TokenKind::GtEq, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Gt, self.scanner.span(start, start + 1))
        }
    }

    fn lex_amp(&mut self, start: usize) -> Lexeme {
        self.scanner.next();
        if self.scanner.match_char('&') {
            return self.lossless_normal(Token::new(
                TokenKind::AndAnd,
                self.scanner.span(start, start + 2),
            ));
        }
        let span = self.scanner.span(start, start + 1);
        self.diagnostics.push(
            self.scanner
                .unexpected_character_diagnostic(span, "unexpected '&'"),
        );
        self.lossless_unknown(span)
    }

    fn lex_pipe(&mut self, start: usize) -> Lexeme {
        self.scanner.next();
        if self.scanner.match_char('|') {
            return self.lossless_normal(Token::new(
                TokenKind::OrOr,
                self.scanner.span(start, start + 2),
            ));
        }
        let span = self.scanner.span(start, start + 1);
        self.diagnostics.push(
            self.scanner
                .unexpected_character_diagnostic(span, "unexpected '|'"),
        );
        self.lossless_unknown(span)
    }

    fn lex_dot(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('.') {
            Token::new(TokenKind::DotDot, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Dot, self.scanner.span(start, start + 1))
        }
    }

    fn lex_colon(&mut self, start: usize) -> Token {
        self.scanner.next();
        if self.scanner.match_char('=') {
            Token::new(TokenKind::ColonEq, self.scanner.span(start, start + 2))
        } else {
            Token::new(TokenKind::Colon, self.scanner.span(start, start + 1))
        }
    }

    fn lossless_normal(&self, token: Token) -> Lexeme {
        let text = self.scanner.text(token.span);
        Lexeme::new(LexemeKind::Token(token.kind), token.span, text)
    }

    fn lossless_unknown(&self, span: Span) -> Lexeme {
        Lexeme::new(LexemeKind::Unknown, span, self.scanner.text(span))
    }

    fn lex_whitespace(&mut self) -> Lexeme {
        let span = self.scanner.skip_whitespace();
        Lexeme::new(LexemeKind::Whitespace, span, self.scanner.text(span))
    }

    fn lex_line_comment(&mut self) -> Lexeme {
        let span = self.scanner.skip_line_comment();
        let text = self.scanner.text(span);
        let kind = if text.starts_with("///") {
            LexemeKind::DocComment
        } else if text.starts_with("//!") {
            LexemeKind::InnerDocComment
        } else {
            LexemeKind::LineComment
        };
        Lexeme::new(kind, span, text)
    }
}
