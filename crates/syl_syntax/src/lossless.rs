use syl_span::Span;

/// A complete syntax tree that preserves every token and all trivia
/// (whitespace, comments) as they appear in the source.
///
/// Unlike the typed AST (`AstFile`), the lossless tree stores exact
/// token text and can reproduce the original source file exactly.
/// Used by the formatter and language server.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessSyntaxFile {
    root: LosslessSyntaxNode,
    tokens: Vec<LosslessToken>,
}

impl LosslessSyntaxFile {
    /// Creates a new lossless file with a root node and token list.
    pub fn new(root: LosslessSyntaxNode, tokens: Vec<LosslessToken>) -> Self {
        debug_assert!(matches!(root.kind(), LosslessNodeKind::File));
        Self { root, tokens }
    }

    /// Returns the root syntax node of this file.
    pub fn root(&self) -> &LosslessSyntaxNode {
        &self.root
    }

    /// Returns every token in order, including trivia tokens.
    pub fn tokens(&self) -> &[LosslessToken] {
        &self.tokens
    }

    /// Writes the exact source text reconstructed from tokens into `out`.
    pub fn write_source(&self, out: &mut String) {
        self.root.write_source(out);
    }

    /// Reconstructs and returns the exact source text.
    pub fn source_text(&self) -> String {
        let mut source =
            String::with_capacity(self.tokens.iter().map(|token| token.text.len()).sum());
        self.write_source(&mut source);
        source
    }
}

/// A single node in the lossless syntax tree, representing a non-terminal
/// with a kind, source span, and ordered children (nodes or tokens).
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessSyntaxNode {
    kind: LosslessNodeKind,
    span: Span,
    children: Vec<LosslessSyntaxElement>,
}

impl LosslessSyntaxNode {
    /// Creates a new syntax node.
    pub fn new(kind: LosslessNodeKind, span: Span, children: Vec<LosslessSyntaxElement>) -> Self {
        Self {
            kind,
            span,
            children,
        }
    }

    /// Returns the kind of this node (File, Item, or Trivia).
    pub fn kind(&self) -> &LosslessNodeKind {
        &self.kind
    }

    /// Returns the combined source span covering all children.
    pub fn span(&self) -> Span {
        self.span
    }

    /// Returns the ordered child elements (tokens and sub-nodes).
    pub fn children(&self) -> &[LosslessSyntaxElement] {
        &self.children
    }

    fn write_source(&self, out: &mut String) {
        for child in &self.children {
            child.write_source(out);
        }
    }
}

/// Either a non-terminal node or a terminal token in the lossless tree.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessSyntaxElement {
    Node(LosslessSyntaxNode),
    Token(LosslessToken),
}

impl LosslessSyntaxElement {
    /// Returns the source span of this element.
    pub fn span(&self) -> Span {
        match self {
            Self::Node(node) => node.span(),
            Self::Token(token) => token.span,
        }
    }

    fn write_source(&self, out: &mut String) {
        match self {
            Self::Node(node) => node.write_source(out),
            Self::Token(token) => out.push_str(token.text.as_ref()),
        }
    }
}

/// The kind of a lossless syntax node.
///
/// `File` — the root of the source file.
/// `Item(item_kind)` — a top-level declaration.
/// `Trivia` — whitespace, comments, or other non-semantic content.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessNodeKind {
    File,
    Item(LosslessItemKind),
    Trivia,
}

/// Which top-level declaration kind a lossless item node represents.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessItemKind {
    Use,
    Const,
    Fn,
    Enum,
    Bundle,
    Interface,
    Map,
    Cell,
    ExternCell,
    Error,
}

/// A single token in the lossless tree, including trivia tokens.
///
/// Unlike `Token`, this stores the exact source text as a string.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessToken {
    pub kind: LosslessTokenKind,
    pub span: Span,
    pub text: Box<str>,
}

impl LosslessToken {
    /// Creates a new lossless token with the given kind, span, and text.
    pub fn new(kind: LosslessTokenKind, span: Span, text: impl Into<Box<str>>) -> Self {
        Self {
            kind,
            span,
            text: text.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessTokenKind {
    /// Keywords such as `fn`, `if`, or `select`.
    Keyword,
    /// Identifiers.
    Ident,
    /// Integer literals.
    Int,
    /// Boolean literals.
    Bool,
    /// String literals.
    Str,
    /// Punctuation and operator tokens.
    Punctuation,
    /// Whitespace kept by the lossless lexer.
    Whitespace,
    /// Line comments kept by the lossless lexer.
    LineComment,
    /// Outer documentation comments kept by the lossless lexer.
    DocComment,
    /// File-level documentation comments kept by the lossless lexer.
    InnerDocComment,
    /// Unknown or unsupported input.
    Unknown,
}
