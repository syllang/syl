pub use crate::token::{Token, TokenKind};
use syl_span::{Diagnostic, SourceId, Span};

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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessTokenKind {
    Token(TokenKind),
    Whitespace,
    LineComment,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessToken {
    pub kind: LosslessTokenKind,
    pub span: Span,
    pub text: Box<str>,
}

impl LosslessToken {
    pub fn new(kind: LosslessTokenKind, span: Span, text: impl Into<Box<str>>) -> Self {
        Self {
            kind,
            span,
            text: text.into(),
        }
    }

    pub fn into_token(self) -> Option<Token> {
        match self.kind {
            LosslessTokenKind::Token(kind) => Some(Token::new(kind, self.span)),
            LosslessTokenKind::Whitespace
            | LosslessTokenKind::LineComment
            | LosslessTokenKind::Unknown => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessLexOutput {
    pub tokens: Vec<LosslessToken>,
    pub diagnostics: Vec<Diagnostic>,
}

impl LosslessLexOutput {
    pub fn new(tokens: Vec<LosslessToken>, diagnostics: Vec<Diagnostic>) -> Self {
        Self {
            tokens,
            diagnostics,
        }
    }
}

#[non_exhaustive]
pub struct Lexer<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    diagnostics: Vec<Diagnostic>,
    source_id: SourceId,
}

impl<'a> Lexer<'a> {
    pub fn new(source: &'a str) -> Self {
        Self::new_in(source, SourceId::default())
    }

    pub fn new_in(source: &'a str, source_id: SourceId) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            diagnostics: Vec::new(),
            source_id,
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
        let output = self.lex_lossless_partial();
        let tokens = output
            .tokens
            .into_iter()
            .filter_map(LosslessToken::into_token)
            .collect();
        LexOutput::new(tokens, output.diagnostics)
    }

    pub fn lex_lossless(&mut self) -> Result<Vec<LosslessToken>, Vec<Diagnostic>> {
        let output = self.lex_lossless_partial();
        if output.diagnostics.is_empty() {
            Ok(output.tokens)
        } else {
            Err(output.diagnostics)
        }
    }

    pub fn lex_lossless_partial(&mut self) -> LosslessLexOutput {
        let mut tokens = Vec::new();
        while let Some(&(i, ch)) = self.chars.peek() {
            match ch {
                c if c.is_whitespace() => {
                    tokens.push(self.lex_whitespace());
                }
                '/' if self.peek_next('/') => {
                    tokens.push(self.lex_line_comment());
                }
                '0'..='9' => {
                    let token = self.lex_number();
                    tokens.push(self.lossless_normal(token));
                }
                '"' => match self.lex_string() {
                    Ok(token) => tokens.push(self.lossless_normal(token)),
                    Err(mut diagnostics) => {
                        let end = self
                            .chars
                            .peek()
                            .map(|(index, _)| *index)
                            .unwrap_or(self.source.len());
                        let span = self.span(i, end);
                        self.diagnostics.append(&mut diagnostics);
                        tokens.push(self.lossless_unknown(span));
                    }
                },
                'a'..='z' | 'A'..='Z' | '_' => {
                    let token = self.lex_ident_or_keyword();
                    tokens.push(self.lossless_normal(token));
                }
                '+' => {
                    let token = self.single(TokenKind::Plus);
                    tokens.push(self.lossless_normal(token));
                }
                '-' => {
                    self.chars.next();
                    if self.match_char('>') {
                        let token = Token::new(TokenKind::Arrow, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Minus, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                '*' => {
                    let token = self.single(TokenKind::Star);
                    tokens.push(self.lossless_normal(token));
                }
                '%' => {
                    let token = self.single(TokenKind::Percent);
                    tokens.push(self.lossless_normal(token));
                }
                '=' => {
                    self.chars.next();
                    if self.match_char('=') {
                        let token = Token::new(TokenKind::EqEq, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else if self.match_char('>') {
                        let token = Token::new(TokenKind::EqGt, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Eq, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                '!' => {
                    self.chars.next();
                    if self.match_char('=') {
                        let token = Token::new(TokenKind::BangEq, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Bang, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                '<' => {
                    self.chars.next();
                    if self.match_char('=') {
                        let token = Token::new(TokenKind::LtEq, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else if self.match_char('<') {
                        let token = Token::new(TokenKind::LtLt, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Lt, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                '>' => {
                    self.chars.next();
                    if self.match_char('=') {
                        let token = Token::new(TokenKind::GtEq, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Gt, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                '&' => {
                    self.chars.next();
                    if self.match_char('&') {
                        let token = Token::new(TokenKind::AndAnd, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let span = self.span(i, i + 1);
                        self.diagnostics
                            .push(self.diagnostic(span, "unexpected '&'"));
                        tokens.push(self.lossless_unknown(span));
                    }
                }
                '|' => {
                    self.chars.next();
                    if self.match_char('|') {
                        let token = Token::new(TokenKind::OrOr, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let span = self.span(i, i + 1);
                        self.diagnostics
                            .push(self.diagnostic(span, "unexpected '|'"));
                        tokens.push(self.lossless_unknown(span));
                    }
                }
                '@' => {
                    let token = self.single(TokenKind::At);
                    tokens.push(self.lossless_normal(token));
                }
                '.' => {
                    self.chars.next();
                    if self.match_char('.') {
                        let token = Token::new(TokenKind::DotDot, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Dot, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                ',' => {
                    let token = self.single(TokenKind::Comma);
                    tokens.push(self.lossless_normal(token));
                }
                ':' => {
                    self.chars.next();
                    if self.match_char('=') {
                        let token = Token::new(TokenKind::ColonEq, self.span(i, i + 2));
                        tokens.push(self.lossless_normal(token));
                    } else {
                        let token = Token::new(TokenKind::Colon, self.span(i, i + 1));
                        tokens.push(self.lossless_normal(token));
                    }
                }
                ';' => {
                    let token = self.single(TokenKind::Semi);
                    tokens.push(self.lossless_normal(token));
                }
                '(' => {
                    let token = self.single(TokenKind::LParen);
                    tokens.push(self.lossless_normal(token));
                }
                ')' => {
                    let token = self.single(TokenKind::RParen);
                    tokens.push(self.lossless_normal(token));
                }
                '{' => {
                    let token = self.single(TokenKind::LBrace);
                    tokens.push(self.lossless_normal(token));
                }
                '}' => {
                    let token = self.single(TokenKind::RBrace);
                    tokens.push(self.lossless_normal(token));
                }
                '[' => {
                    let token = self.single(TokenKind::LBracket);
                    tokens.push(self.lossless_normal(token));
                }
                ']' => {
                    let token = self.single(TokenKind::RBracket);
                    tokens.push(self.lossless_normal(token));
                }
                _ => {
                    self.chars.next();
                    let span = self.span(i, i + ch.len_utf8());
                    self.diagnostics
                        .push(self.diagnostic(span, format!("unexpected character {ch:?}")));
                    tokens.push(self.lossless_unknown(span));
                }
            }
        }
        LosslessLexOutput::new(tokens, std::mem::take(&mut self.diagnostics))
    }

    fn single(&mut self, kind: TokenKind) -> Token {
        let Some((i, ch)) = self.chars.next() else {
            return Token::new(kind, Span::default());
        };
        Token::new(kind, self.span(i, i + ch.len_utf8()))
    }

    fn lex_number(&mut self) -> Token {
        let Some((start, _)) = self.chars.peek().copied() else {
            return Token::new(TokenKind::Int(0), Span::default());
        };
        let mut end = start;
        let mut value = 0u64;
        while let Some(&(i, ch)) = self.chars.peek() {
            if ch.is_ascii_digit() {
                let Some(digit) = ch.to_digit(10) else {
                    break;
                };
                value = value.saturating_mul(10).saturating_add(u64::from(digit));
                end = i + ch.len_utf8();
                self.chars.next();
            } else {
                break;
            }
        }
        Token::new(TokenKind::Int(value), self.span(start, end))
    }

    fn lex_ident_or_keyword(&mut self) -> Token {
        let Some((start, _)) = self.chars.peek().copied() else {
            return Token::new(TokenKind::Ident(String::new()), Span::default());
        };
        let mut end = start;
        let mut ident = String::new();
        while let Some(&(i, ch)) = self.chars.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ident.push(ch);
                end = i + ch.len_utf8();
                self.chars.next();
            } else {
                break;
            }
        }
        let kind = match ident.as_str() {
            "const" => TokenKind::KwConst,
            "use" => TokenKind::KwUse,
            "fn" => TokenKind::KwFn,
            "let" => TokenKind::KwLet,
            "return" => TokenKind::KwReturn,
            "this" => TokenKind::KwThis,
            "var" => TokenKind::KwVar,
            "for" => TokenKind::KwFor,
            "while" => TokenKind::KwWhile,
            "if" => TokenKind::KwIf,
            "else" => TokenKind::KwElse,
            "enum" => TokenKind::KwEnum,
            "bundle" => TokenKind::KwBundle,
            "interface" => TokenKind::KwInterface,
            "view" => TokenKind::KwView,
            "match" => TokenKind::KwMatch,
            "select" => TokenKind::KwSelect,
            "compile_error" => TokenKind::Ident(ident),
            "priority" => TokenKind::KwPriority,
            "unique" => TokenKind::KwUnique,
            "map" => TokenKind::KwMap,
            "cell" => TokenKind::KwCell,
            "inplace" => TokenKind::KwInplace,
            "extern" => TokenKind::KwExtern,
            "signal" => TokenKind::KwSignal,
            "reg" => TokenKind::KwReg,
            "place" => TokenKind::KwPlace,
            "next" => TokenKind::KwNext,
            "in" => TokenKind::KwIn,
            "inout" => TokenKind::KwInOut,
            "out" => TokenKind::KwOut,
            "and" => TokenKind::KwAnd,
            "or" => TokenKind::KwOr,
            "not" => TokenKind::KwNot,
            "xor" => TokenKind::KwXor,
            "eq" => TokenKind::KwEqWord,
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            _ => TokenKind::Ident(ident),
        };
        Token::new(kind, self.span(start, end))
    }

    fn lex_string(&mut self) -> Result<Token, Vec<Diagnostic>> {
        let Some((start, _)) = self.chars.next() else {
            self.diagnostics
                .push(self.string_diagnostic(Span::default(), "unterminated string literal"));
            return Err(std::mem::take(&mut self.diagnostics));
        };
        let mut value = String::new();
        while let Some((i, ch)) = self.chars.next() {
            if ch == '"' {
                return Ok(Token::new(TokenKind::Str(value), self.span(start, i + 1)));
            }
            if ch == '\\' {
                match self.chars.next() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((j, other)) => {
                        self.diagnostics.push(self.string_escape_diagnostic(
                            self.span(j, j + other.len_utf8()),
                            "unsupported string escape",
                        ));
                        return Err(std::mem::take(&mut self.diagnostics));
                    }
                    None => {
                        self.diagnostics.push(self.string_diagnostic(
                            self.span(start, start + 1),
                            "unterminated string literal",
                        ));
                        return Err(std::mem::take(&mut self.diagnostics));
                    }
                }
            } else {
                value.push(ch);
            }
        }
        self.diagnostics.push(
            self.string_diagnostic(self.span(start, start + 1), "unterminated string literal"),
        );
        Err(std::mem::take(&mut self.diagnostics))
    }

    fn diagnostic(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(span, message)
            .with_code("E_SYNTAX_UNEXPECTED_CHARACTER")
            .with_source("syl_syntax::lexer")
    }

    fn string_diagnostic(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(span, message)
            .with_code("E_SYNTAX_UNTERMINATED_STRING")
            .with_source("syl_syntax::lexer")
    }

    fn string_escape_diagnostic(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(span, message)
            .with_code("E_SYNTAX_UNSUPPORTED_STRING_ESCAPE")
            .with_source("syl_syntax::lexer")
    }

    fn span(&self, start: usize, end: usize) -> Span {
        Span::new_in(self.source_id, start, end)
    }

    fn text(&self, span: Span) -> Box<str> {
        self.source
            .get(span.start..span.end)
            .unwrap_or_default()
            .into()
    }

    fn lossless_normal(&self, token: Token) -> LosslessToken {
        let text = self.text(token.span);
        LosslessToken::new(LosslessTokenKind::Token(token.kind), token.span, text)
    }

    fn lossless_unknown(&self, span: Span) -> LosslessToken {
        LosslessToken::new(LosslessTokenKind::Unknown, span, self.text(span))
    }

    fn lex_whitespace(&mut self) -> LosslessToken {
        let Some((start, _)) = self.chars.peek().copied() else {
            return LosslessToken::new(LosslessTokenKind::Whitespace, Span::default(), "");
        };
        let mut end = start;
        while let Some(&(i, ch)) = self.chars.peek() {
            if ch.is_whitespace() {
                end = i + ch.len_utf8();
                self.chars.next();
            } else {
                break;
            }
        }
        let span = self.span(start, end);
        LosslessToken::new(LosslessTokenKind::Whitespace, span, self.text(span))
    }

    fn peek_next(&mut self, expected: char) -> bool {
        let mut iter = self.chars.clone();
        iter.next();
        matches!(iter.next(), Some((_, ch)) if ch == expected)
    }

    fn match_char(&mut self, expected: char) -> bool {
        matches!(self.chars.peek(), Some(&(_, ch)) if ch == expected) && self.chars.next().is_some()
    }

    fn lex_line_comment(&mut self) -> LosslessToken {
        let Some((start, _)) = self.chars.peek().copied() else {
            return LosslessToken::new(LosslessTokenKind::LineComment, Span::default(), "");
        };
        let mut end = start;
        while let Some(&(i, ch)) = self.chars.peek() {
            if ch == '\n' {
                break;
            }
            end = i + ch.len_utf8();
            self.chars.next();
        }
        let span = self.span(start, end);
        LosslessToken::new(LosslessTokenKind::LineComment, span, self.text(span))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lexes_sample_expression() {
        let tokens = Lexer::new("const X = a + b * 3;").lex_all().unwrap();
        assert!(!tokens.is_empty());
        assert!(matches!(tokens[0].kind, TokenKind::KwConst));
    }

    #[test]
    fn lossless_lexer_preserves_whitespace_and_line_comments() {
        let source = "const X = 1; // keep me\nconst Y = 2;";
        let tokens = Lexer::new(source).lex_lossless().unwrap();
        let facts: Vec<_> = tokens
            .iter()
            .map(|token| {
                (
                    &token.kind,
                    token.span.start,
                    token.span.end,
                    token.text.as_ref(),
                )
            })
            .collect();

        assert!(matches!(
            facts[0].0,
            LosslessTokenKind::Token(TokenKind::KwConst)
        ));
        assert!(matches!(facts[1].0, LosslessTokenKind::Whitespace));
        assert_eq!(facts[1].3, " ");
        let comment = tokens
            .iter()
            .find(|token| matches!(token.kind, LosslessTokenKind::LineComment))
            .expect("line comment should be retained as trivia");
        assert_eq!(comment.text.as_ref(), "// keep me");
        assert_eq!(comment.span.start, source.find("//").unwrap());
        assert_eq!(comment.span.end, comment.span.start + "// keep me".len());
        assert!(matches!(
            tokens
                .iter()
                .find(|token| token.text.as_ref() == "\n")
                .map(|token| &token.kind),
            Some(LosslessTokenKind::Whitespace)
        ));
    }
}
