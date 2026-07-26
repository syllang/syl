use crate::lexer::{Lexer, LosslessLexer, Token, TokenKind};
use crate::*;
use std::collections::{HashMap, HashSet};
use syl_span::{Diagnostic, SourceId, Span};

mod doc;
mod expr;
mod item;
mod lossless_tree;
mod output;
mod recovery;
mod span_ext;
mod stmt;
mod type_expr;

pub use output::ParseOutput;

/// Whether a `{ ... }` body is a software function body or hardware cell body.
///
/// Affects assignment operators (`=` vs `:=`) and drive vs assign statements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum BlockContext {
    Function,
    Hardware,
}

/// One entry inside a block: a statement, or a trailing expression value.
#[derive(Debug)]
pub(super) enum BlockEntry {
    Stmt(Box<Stmt>),
    Tail(Expr),
}

/// Entry-point for parsing a `.syl` source string into a typed AST.
///
/// `SourceParser` takes a source string and optionally a `SourceId`,
/// then drives the lexer and parser. Use `parse_file` for simple
/// one-shot parsing, or `parse_file_partial` to inspect warnings even
/// when errors are present.
///
/// # Main usage flow
///
/// ```text
///  source: &str  (+ optional SourceId)
///              |
///              v
///    +---------------------+
///    |    SourceParser     |   new / new_in
///    +----------+----------+
///               |
///     +---------+---------------------------+
///     |                   |                 |
///     v                   v                 v
///  parse_file()    parse_file_partial()  parse_file_with_lossless()
///  Result<AstFile> ParseOutput           (ParseOutput, LosslessSyntaxFile)
///
///  parse_expr()  -->  Result<Expr>   (standalone expression)
/// ```
///
/// Internally: lex (lossless) → prepare docs → [`Parser`] → AST (+ optional CST).
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SourceParser<'a> {
    source: &'a str,
    source_id: SourceId,
}

impl<'a> SourceParser<'a> {
    /// Creates a parser for the given source with a default `SourceId`.
    pub fn new(source: &'a str) -> Self {
        Self::new_in(source, SourceId::default())
    }

    /// Creates a parser for the given source with an explicit source identity.
    pub fn new_in(source: &'a str, source_id: SourceId) -> Self {
        Self { source, source_id }
    }

    /// Parses the source into a complete AST, returning an error on the first
    /// diagnostic. Use `parse_file_partial` to inspect all diagnostics.
    pub fn parse_file(&self) -> Result<AstFile, Vec<Diagnostic>> {
        self.parse_file_partial().into_result()
    }

    /// Parses the source and returns both the best-effort AST and any
    /// diagnostics collected during lexing and parsing.
    pub fn parse_file_partial(&self) -> ParseOutput {
        let mut lexer = LosslessLexer::new_in(self.source, self.source_id);
        let output = lexer.lex_all_partial();
        let prepared = doc::prepare_lexemes(output.lexemes);
        let mut parsed = Parser::new_at_end_with_docs(
            prepared.tokens,
            self.source_id,
            self.source.len(),
            prepared.doc_comments,
            prepared.module_doc,
        )
        .parse_file_partial_with_source(self.source);
        parsed.diagnostics.extend(output.diagnostics);
        parsed.diagnostics.extend(prepared.diagnostics);
        parsed
    }

    /// Parses the source, returning the parse output together with a
    /// trivia-preserving lossless syntax tree (for formatting, LSP, etc.).
    pub fn parse_file_with_lossless(&self) -> (ParseOutput, LosslessSyntaxFile) {
        let mut lexer = LosslessLexer::new_in(self.source, self.source_id);
        let output = lexer.lex_all_partial();
        let prepared = doc::prepare_lexemes(output.lexemes);
        let syntax_tokens = prepared.syntax_tokens.clone();
        let mut parsed = Parser::new_at_end_with_docs(
            prepared.tokens,
            self.source_id,
            self.source.len(),
            prepared.doc_comments,
            prepared.module_doc,
        )
        .parse_file_partial_with_source(self.source);
        parsed.diagnostics.extend(output.diagnostics);
        parsed.diagnostics.extend(prepared.diagnostics);
        let syntax = lossless_tree::build_lossless_syntax_file(
            self.source_id,
            self.source.len(),
            &parsed.file,
            syntax_tokens,
        );
        (parsed, syntax)
    }

    /// Parses the source as a standalone expression (not a full file).
    /// Useful for REPL, test helpers, or incremental compilation.
    pub fn parse_expr(&self) -> Result<Expr, Vec<Diagnostic>> {
        let tokens = Lexer::new_in(self.source, self.source_id).lex_all()?;
        Parser::new_at_end(tokens, self.source_id, self.source.len()).parse_expr(0)
    }
}

/// Recursive-descent parser over a token stream.
///
/// Not the public entry point — prefer [`SourceParser`]. This type owns cursor
/// state, diagnostics, doc attachment, and block/mutable-local context.
///
/// Methods are split by concern across submodules:
///
/// ```text
///  parser.rs       SourceParser, Parser state, file entry, token primitives
///  item.rs         top-level items (use/const/fn/cell/struct/...)
///  stmt.rs         statements + block bodies
///  expr.rs         expressions / patterns
///  type_expr.rs    types, generics, params, field/view bodies
///  doc.rs          doc comment collection + attachment
///  recovery.rs     error recovery boundaries
/// ```
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    eof_span: Span,
    block_context: BlockContext,
    mutable_local_scopes: Vec<HashSet<String>>,
    doc_comments: HashMap<usize, doc::CollectedDoc>,
    module_doc: Option<String>,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    #[cfg(test)]
    fn new(tokens: Vec<Token>) -> Self {
        let eof_span = tokens
            .last()
            .map(|token| Span::new_in(token.span.source, token.span.end, token.span.end))
            .unwrap_or_default();
        Self::new_with_eof(tokens, eof_span)
    }

    fn new_at_end(tokens: Vec<Token>, source_id: SourceId, source_len: usize) -> Self {
        Self::new_with_eof(tokens, Span::new_in(source_id, source_len, source_len))
    }

    fn new_at_end_with_docs(
        tokens: Vec<Token>,
        source_id: SourceId,
        source_len: usize,
        doc_comments: HashMap<usize, doc::CollectedDoc>,
        module_doc: Option<String>,
    ) -> Self {
        Self::new_with_eof_and_docs(
            tokens,
            Span::new_in(source_id, source_len, source_len),
            doc_comments,
            module_doc,
        )
    }

    fn new_with_eof(tokens: Vec<Token>, eof_span: Span) -> Self {
        Self::new_with_eof_and_docs(tokens, eof_span, HashMap::new(), None)
    }

    fn new_with_eof_and_docs(
        tokens: Vec<Token>,
        eof_span: Span,
        doc_comments: HashMap<usize, doc::CollectedDoc>,
        module_doc: Option<String>,
    ) -> Self {
        Self {
            tokens,
            pos: 0,
            eof_span,
            block_context: BlockContext::Function,
            mutable_local_scopes: Vec::new(),
            doc_comments,
            module_doc,
            diagnostics: Vec::new(),
        }
    }

    pub fn parse_file(self) -> Result<AstFile, Vec<Diagnostic>> {
        self.parse_file_partial().into_result()
    }

    pub fn parse_file_partial(mut self) -> ParseOutput {
        let (file, diagnostics) = self.parse_file_parts();
        ParseOutput::new(file, diagnostics)
    }

    pub(crate) fn parse_file_partial_with_source(mut self, source: &str) -> ParseOutput {
        let (file, diagnostics) = self.parse_file_parts();
        let node_index = file.build_node_index(source);
        ParseOutput::with_node_index(file, diagnostics, node_index)
    }

    fn parse_file_parts(&mut self) -> (AstFile, Vec<Diagnostic>) {
        let mut items = Vec::new();
        while !self.is_eof() {
            let start_pos = self.pos;
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(mut diagnostics) => {
                    self.diagnostics.append(&mut diagnostics);
                    let span = self.recover_item_boundary(start_pos);
                    items.push(Item::Error(ErrorItem::new(span)));
                }
            }
        }
        for (_, doc) in std::mem::take(&mut self.doc_comments) {
            self.error(
                doc.span,
                "`///` doc comment must attach to a following declaration",
            );
        }
        (
            AstFile::with_source_doc(self.eof_span.source, self.module_doc.take(), items),
            std::mem::take(&mut self.diagnostics),
        )
    }

    // --- token cursor primitives ---

    pub(super) fn expect_ident(&mut self) -> Result<String, Vec<Diagnostic>> {
        match self.bump() {
            Some(Token {
                kind: TokenKind::Ident(name),
                ..
            }) => Ok(name),
            Some(tok) => {
                self.error(tok.span, "expected identifier");
                Err(std::mem::take(&mut self.diagnostics))
            }
            None => {
                self.error(self.eof_span(), "unexpected end of source");
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }

    pub(super) fn expect(&mut self, kind: TokenKind) -> Result<Token, Vec<Diagnostic>> {
        match self.bump() {
            Some(tok) if tok.kind == kind => Ok(tok),
            Some(tok) => {
                self.error(tok.span, format!("expected {:?}", kind));
                Err(std::mem::take(&mut self.diagnostics))
            }
            None => {
                self.error(self.eof_span(), format!("expected {:?}", kind));
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }

    pub(super) fn consume(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.check(kind) { self.bump() } else { None }
    }

    pub(super) fn check(&self, kind: &TokenKind) -> bool {
        self.peek_kind() == Some(kind)
    }

    pub(super) fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }

    pub(super) fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }

    pub(super) fn bump(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }

    pub(super) fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    pub(super) fn eof_span(&self) -> Span {
        self.eof_span
    }

    pub(super) fn block_context(&self) -> BlockContext {
        self.block_context
    }

    pub(super) fn is_mutable_local(&self, name: &str) -> bool {
        self.mutable_local_scopes
            .iter()
            .rev()
            .any(|scope| scope.contains(name))
    }

    pub(super) fn declare_mutable_local(&mut self, name: &str) {
        if let Some(scope) = self.mutable_local_scopes.last_mut() {
            scope.insert(name.to_owned());
        }
    }

    pub(super) fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or_default()
    }

    pub(super) fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(
            Diagnostic::new(span, message)
                .with_code("E_SYNTAX_PARSE")
                .with_source("syl_syntax::parser"),
        );
    }
}

#[cfg(test)]
mod tests;
