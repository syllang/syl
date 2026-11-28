//! Parse message literals while preserving the source coordinates of captures.
use super::{Lexer, Parser};
use crate::{CallArg, Expr};
use syl_span::{Diagnostic, Span};

pub type FormatTemplate<T> = Result<Vec<FormatPart<T>>, Box<Diagnostic>>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatSpec {
    Default,
    Unsigned,
    Signed,
    Debug,
    Hex {
        upper: bool,
        prefix: bool,
        digits: u16,
    },
    Binary {
        prefix: bool,
        digits: u16,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormatPart<T> {
    Literal(String),
    Argument { index: usize, spec: FormatSpec },
    Capture { expr: T, spec: FormatSpec },
}

impl<T> FormatPart<T> {
    pub fn map<U>(&self, f: impl FnOnce(&T) -> U) -> FormatPart<U> {
        match self {
            Self::Literal(text) => FormatPart::Literal(text.clone()),
            Self::Argument { index, spec } => FormatPart::Argument {
                index: *index,
                spec: *spec,
            },
            Self::Capture { expr, spec } => FormatPart::Capture {
                expr: f(expr),
                spec: *spec,
            },
        }
    }
}

impl Parser {
    pub(super) fn attach_format(&self, callee: &Expr, args: &mut [CallArg]) {
        let mut root = callee;
        while let Expr::Group(base, _) | Expr::GenericApp { callee: base, .. } = root {
            root = base;
        }
        if let Expr::Ident(name, _) = root
            && matches!(name.as_str(), "assert" | "log")
            && let Some(arg) = args
                .iter_mut()
                .filter(|a| a.name.is_none())
                .nth(usize::from(name == "assert"))
            && let Expr::Str(text, span) = &arg.value
        {
            let escapes = self
                .string_escapes
                .get(&span.start)
                .map_or(&[][..], Vec::as_slice);
            arg.format = Some(parse_format(text, *span, escapes));
        }
    }

    pub(super) fn index_string_escapes(&mut self, source: &str) {
        for token in &self.tokens {
            let super::TokenKind::Str(text) = &token.kind else {
                continue;
            };
            if token.span.end - token.span.start == text.len() + 2 {
                continue;
            }
            let raw = &source[token.span.start + 1..token.span.end - 1];
            let mut escapes = Vec::new();
            let mut chars = raw.char_indices();
            while let Some((i, c)) = chars.next() {
                if c == '\\' {
                    escapes.push(i - escapes.len());
                    chars.next();
                }
            }
            self.string_escapes.insert(token.span.start, escapes);
        }
    }
}

fn parse_format(text: &str, span: Span, escapes: &[usize]) -> FormatTemplate<Expr> {
    let offset = |i| span.start + 1 + i + escapes.partition_point(|&at| at < i);
    let location = |start, end| Span::new_in(span.source, offset(start), offset(end));
    let error = |at, message| Box::new(Diagnostic::new(at, message).with_code("E_SYNTAX_FORMAT"));
    let mut chars = text.char_indices().peekable();
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut implicit = 0;
    while let Some((at, ch)) = chars.next() {
        if ch != '{' && ch != '}' {
            literal.push(ch);
            continue;
        }
        if chars.next_if(|(_, next)| *next == ch).is_some() {
            literal.push(ch);
            continue;
        }
        if ch == '}' {
            return Err(error(
                location(at, at + 1),
                "unmatched '}' in format string",
            ));
        }
        if !literal.is_empty() {
            parts.push(FormatPart::Literal(std::mem::take(&mut literal)));
        }
        let start = at + 1;
        let mut depth = 0usize;
        let mut colon = None;
        let end = loop {
            let Some((i, c)) = chars.next() else {
                return Err(error(location(at, at + 1), "unclosed '{' in format string"));
            };
            match c {
                '}' if depth == 0 => break i,
                '[' | '(' | '{' => depth += 1,
                ']' | ')' | '}' => depth = depth.saturating_sub(1),
                ':' if depth == 0 && colon.is_none() => colon = Some(i),
                _ => {}
            }
        };
        let target = &text[start..colon.unwrap_or(end)];
        let text_spec = colon.map_or("", |i| &text[i + 1..end]);
        let spec = parse_spec(text_spec).ok_or_else(|| {
            Diagnostic::new(
                location(colon.unwrap_or(start), end),
                format!("unknown format specifier '{text_spec}'"),
            )
            .with_code("E_SYNTAX_FORMAT")
        })?;
        let trimmed = target.trim();
        if trimmed.is_empty() || trimmed.bytes().all(|b| b.is_ascii_digit()) {
            let index = if trimmed.is_empty() {
                implicit += 1;
                implicit - 1
            } else {
                trimmed
                    .parse()
                    .map_err(|_| error(location(start, end), "invalid positional argument index"))?
            };
            parts.push(FormatPart::Argument { index, spec });
        } else {
            let mut tokens = Lexer::new_in(target, span.source)
                .lex_all()
                .map_err(|_| error(location(start, end), "invalid expression in format string"))?;
            for token in &mut tokens {
                token.span = location(start + token.span.start, start + token.span.end);
            }
            let mut parser =
                Parser::new_with_eof(tokens, location(start + target.len(), start + target.len()));
            let expr = parser
                .parse_expr(0)
                .map_err(|mut errors| errors.remove(0))?;
            if !parser.is_eof() {
                return Err(error(
                    parser.peek().unwrap().span,
                    "unexpected token in format expression",
                ));
            }
            parts.push(FormatPart::Capture { expr, spec });
        }
    }
    if !literal.is_empty() {
        parts.push(FormatPart::Literal(literal));
    }
    Ok(parts)
}

fn parse_spec(text: &str) -> Option<FormatSpec> {
    Some(match text {
        "" => FormatSpec::Default,
        "d" | "u" => FormatSpec::Unsigned,
        "s" => FormatSpec::Signed,
        "?" => FormatSpec::Debug,
        _ => {
            let prefix = text.starts_with('#');
            let spec = text.strip_prefix('#').unwrap_or(text);
            let padding = spec.strip_suffix(['x', 'X', 'b'])?;
            let digits = if padding.is_empty() {
                0
            } else if padding.starts_with('0') {
                padding
                    .parse::<u16>()
                    .ok()?
                    .saturating_sub(if prefix { 2 } else { 0 })
            } else {
                return None;
            };
            if spec.ends_with('b') {
                FormatSpec::Binary { prefix, digits }
            } else {
                FormatSpec::Hex {
                    upper: spec.ends_with('X'),
                    prefix,
                    digits,
                }
            }
        }
    })
}
