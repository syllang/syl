use super::scanner::Scanner;
use crate::token::{Token, TokenKind};
use syl_span::{Diagnostic, SourceId};

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LexOutput {
    pub tokens: Vec<Token>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LexOutput {
    pub fn new(tokens: Vec<Token>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            tokens,
            diagnostics,
        }
    }
}

#[non_exhaustive]
pub struct Lexer<'a> {
    scanner: Scanner<'a>,
    diagnostics: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self::new_in(source, SourceId::default())
    }

    pub fn new_in(source: &'a str, source_id: SourceId) -> Self {
        Self {
            scanner: Scanner::new_in(source, source_id),
            diagnostics: Vec::new(),
        }
    }

    pub fn lex_all(&mut self) -> Result<Vec<Token>, Vec<Diagnostic>> {
        let output = self.lex_all_partial();
        if output.diagnostics.is_empty() {
            Ok(output.tokens)
        } else {
            Err(output.diagnostics)
        }
    }

    pub fn lex_all_partial(&mut self) -> LexOutput {
        let mut tokens = Vec::new();
        while let Some((i, ch)) = self.scanner.peek() {
            match ch {
                c if c.is_whitespace() => {
                    self.scanner.skip_whitespace();
                }
                '/' if self.scanner.peek_next('/') => {
                    self.scanner.skip_line_comment();
                }
                '/' => tokens.push(self.scanner.single(TokenKind::Slash)),
                '0'..='9' => tokens.push(self.scanner.lex_number()),
                '"' => match self.scanner.lex_string() {
                    Ok(token) => tokens.push(token),
                    Err(mut diagnostics) => self.diagnostics.append(&mut diagnostics),
                },
                'a'..='z' | 'A'..='Z' | '_' => {
                    tokens.push(self.scanner.lex_ident_or_keyword());
                }
                '+' => tokens.push(self.scanner.single(TokenKind::Plus)),
                '-' => tokens.push(self.lex_dash(i)),
                '*' => tokens.push(self.scanner.single(TokenKind::Star)),
                '%' => tokens.push(self.scanner.single(TokenKind::Percent)),
                '=' => tokens.push(self.lex_eq(i)),
                '!' => tokens.push(self.lex_bang(i)),
                '<' => tokens.push(self.lex_lt(i)),
                '>' => tokens.push(self.lex_gt(i)),
                '&' => {
                    if let Some(token) = self.lex_amp(i) {
                        tokens.push(token);
                    }
                }
                '|' => {
                    if let Some(token) = self.lex_pipe(i) {
                        tokens.push(token);
                    }
                }
                '@' => tokens.push(self.scanner.single(TokenKind::At)),
                '.' => tokens.push(self.lex_dot(i)),
                ',' => tokens.push(self.scanner.single(TokenKind::Comma)),
                ':' => tokens.push(self.lex_colon(i)),
                ';' => tokens.push(self.scanner.single(TokenKind::Semi)),
                '(' => tokens.push(self.scanner.single(TokenKind::LParen)),
                ')' => tokens.push(self.scanner.single(TokenKind::RParen)),
                '{' => tokens.push(self.scanner.single(TokenKind::LBrace)),
                '}' => tokens.push(self.scanner.single(TokenKind::RBrace)),
                '[' => tokens.push(self.scanner.single(TokenKind::LBracket)),
                ']' => tokens.push(self.scanner.single(TokenKind::RBracket)),
                _ => {
                    self.scanner.next();
                    let span = self.scanner.span(i, i + ch.len_utf8());
                    self.diagnostics
                        .push(self.scanner.unexpected_character_diagnostic(
                            span,
                            format!("unexpected character {ch:?}"),
                        ));
                }
            }
        }
        LexOutput::new(tokens, std::mem::take(&mut self.diagnostics))
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

    fn lex_amp(&mut self, start: usize) -> Option<Token> {
        self.scanner.next();
        if self.scanner.match_char('&') {
            return Some(Token::new(
                TokenKind::AndAnd,
                self.scanner.span(start, start + 2),
            ));
        }
        let span = self.scanner.span(start, start + 1);
        self.diagnostics.push(
            self.scanner
                .unexpected_character_diagnostic(span, "unexpected '&'"),
        );
        None
    }

    fn lex_pipe(&mut self, start: usize) -> Option<Token> {
        self.scanner.next();
        if self.scanner.match_char('|') {
            return Some(Token::new(
                TokenKind::OrOr,
                self.scanner.span(start, start + 2),
            ));
        }
        let span = self.scanner.span(start, start + 1);
        self.diagnostics.push(
            self.scanner
                .unexpected_character_diagnostic(span, "unexpected '|'"),
        );
        None
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
}
