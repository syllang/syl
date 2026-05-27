use crate::token::{Token, TokenKind};
use syl_span::{Diagnostic, SourceId, Span};

#[non_exhaustive]
pub(super) struct Scanner<'a> {
    source: &'a str,
    chars: std::iter::Peekable<std::str::CharIndices<'a>>,
    source_id: SourceId,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(source: &'a str) -> Self {
        Self::new_in(source, SourceId::default())
    }

    pub(super) fn new_in(source: &'a str, source_id: SourceId) -> Self {
        Self {
            source,
            chars: source.char_indices().peekable(),
            source_id,
        }
    }

    pub(super) fn peek(&mut self) -> Option<(usize, char)> {
        self.chars.peek().copied()
    }

    pub(super) fn next(&mut self) -> Option<(usize, char)> {
        self.chars.next()
    }

    pub(super) fn source_len(&self) -> usize {
        self.source.len()
    }

    pub(super) fn span(&self, start: usize, end: usize) -> Span {
        Span::new_in(self.source_id, start, end)
    }

    pub(super) fn text(&self, span: Span) -> Box<str> {
        self.source
            .get(span.start..span.end)
            .unwrap_or_default()
            .into()
    }

    pub(super) fn single(&mut self, kind: TokenKind) -> Token {
        let Some((i, ch)) = self.next() else {
            return Token::new(kind, Span::default());
        };
        Token::new(kind, self.span(i, i + ch.len_utf8()))
    }

    pub(super) fn lex_number(&mut self) -> Token {
        let Some((start, _)) = self.peek() else {
            return Token::new(TokenKind::Int(0), Span::default());
        };
        let mut end = start;
        let mut value = 0u64;
        while let Some((i, ch)) = self.peek() {
            if ch.is_ascii_digit() {
                let Some(digit) = ch.to_digit(10) else {
                    break;
                };
                value = value.saturating_mul(10).saturating_add(u64::from(digit));
                end = i + ch.len_utf8();
                self.next();
            } else {
                break;
            }
        }
        Token::new(TokenKind::Int(value), self.span(start, end))
    }

    pub(super) fn lex_ident_or_keyword(&mut self) -> Token {
        let Some((start, _)) = self.peek() else {
            return Token::new(TokenKind::Ident(String::new()), Span::default());
        };
        let mut end = start;
        let mut ident = String::new();
        while let Some((i, ch)) = self.peek() {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ident.push(ch);
                end = i + ch.len_utf8();
                self.next();
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

    pub(super) fn lex_string(&mut self) -> Result<Token, Vec<Diagnostic>> {
        let Some((start, _)) = self.next() else {
            return Err(vec![
                self.string_diagnostic(Span::default(), "unterminated string literal"),
            ]);
        };
        let mut value = String::new();
        while let Some((i, ch)) = self.next() {
            if ch == '"' {
                return Ok(Token::new(TokenKind::Str(value), self.span(start, i + 1)));
            }
            if ch == '\\' {
                match self.next() {
                    Some((_, 'n')) => value.push('\n'),
                    Some((_, 't')) => value.push('\t'),
                    Some((_, '"')) => value.push('"'),
                    Some((_, '\\')) => value.push('\\'),
                    Some((j, other)) => {
                        return Err(vec![self.string_escape_diagnostic(
                            self.span(j, j + other.len_utf8()),
                            "unsupported string escape",
                        )]);
                    }
                    None => {
                        return Err(vec![self.string_diagnostic(
                            self.span(start, start + 1),
                            "unterminated string literal",
                        )]);
                    }
                }
            } else {
                value.push(ch);
            }
        }
        Err(vec![self.string_diagnostic(
            self.span(start, start + 1),
            "unterminated string literal",
        )])
    }

    pub(super) fn skip_whitespace(&mut self) -> Span {
        let Some((start, _)) = self.peek() else {
            return Span::default();
        };
        let mut end = start;
        while let Some((i, ch)) = self.peek() {
            if ch.is_whitespace() {
                end = i + ch.len_utf8();
                self.next();
            } else {
                break;
            }
        }
        self.span(start, end)
    }

    pub(super) fn skip_line_comment(&mut self) -> Span {
        let Some((start, _)) = self.peek() else {
            return Span::default();
        };
        let mut end = start;
        while let Some((i, ch)) = self.peek() {
            if ch == '\n' {
                break;
            }
            end = i + ch.len_utf8();
            self.next();
        }
        self.span(start, end)
    }

    pub(super) fn peek_next(&mut self, expected: char) -> bool {
        let mut iter = self.chars.clone();
        iter.next();
        matches!(iter.next(), Some((_, ch)) if ch == expected)
    }

    pub(super) fn match_char(&mut self, expected: char) -> bool {
        matches!(self.chars.peek(), Some(&(_, ch)) if ch == expected) && self.next().is_some()
    }

    pub(super) fn unexpected_character_diagnostic(
        &self,
        span: Span,
        message: impl Into<String>,
    ) -> Diagnostic {
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
}
