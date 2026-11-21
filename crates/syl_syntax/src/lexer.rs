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
mod tests;
