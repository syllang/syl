//! Lexical analysis for Syl source.
//!
//! The semantic lexer produces syntax-significant `Token`s only. The lossless
//! lexer produces source `Lexeme`s, including trivia and unknown fragments.

mod lossless;
mod scanner;
mod semantic;

pub use crate::token::{Token, TokenKind};
pub use lossless::{Lexeme, LexemeKind, LosslessLexOutput, LosslessLexer};
pub use semantic::{LexOutput, Lexer};

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
    fn semantic_lexer_skips_trivia() {
        let tokens = Lexer::new("const X = 1; // keep me\nconst Y = 2;")
            .lex_all()
            .unwrap();

        assert!(
            tokens
                .iter()
                .all(|token| !matches!(token.kind, TokenKind::Ident(ref text) if text == "keep"))
        );
        assert_eq!(
            tokens
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::KwConst))
                .count(),
            2
        );
    }

    #[test]
    fn lossless_lexer_preserves_whitespace_and_line_comments() {
        let source = "const X = 1; // keep me\nconst Y = 2;";
        let lexemes = LosslessLexer::new(source).lex_all().unwrap();
        let facts: Vec<_> = lexemes
            .iter()
            .map(|lexeme| {
                (
                    &lexeme.kind,
                    lexeme.span.start,
                    lexeme.span.end,
                    lexeme.text.as_ref(),
                )
            })
            .collect();

        assert!(matches!(facts[0].0, LexemeKind::Token(TokenKind::KwConst)));
        assert!(matches!(facts[1].0, LexemeKind::Whitespace));
        assert_eq!(facts[1].3, " ");
        let comment = lexemes
            .iter()
            .find(|lexeme| matches!(lexeme.kind, LexemeKind::LineComment))
            .expect("line comment should be retained as trivia");
        assert_eq!(comment.text.as_ref(), "// keep me");
        assert_eq!(comment.span.start, source.find("//").unwrap());
        assert_eq!(comment.span.end, comment.span.start + "// keep me".len());
        assert!(matches!(
            lexemes
                .iter()
                .find(|lexeme| lexeme.text.as_ref() == "\n")
                .map(|lexeme| &lexeme.kind),
            Some(LexemeKind::Whitespace)
        ));
    }
}
