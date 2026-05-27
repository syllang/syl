use syl_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessSyntaxFile {
    root: LosslessSyntaxNode,
    tokens: Vec<LosslessToken>,
}

impl LosslessSyntaxFile {
    pub fn new(root: LosslessSyntaxNode, tokens: Vec<LosslessToken>) -> Self {
        debug_assert!(matches!(root.kind(), LosslessNodeKind::File));
        Self { root, tokens }
    }

    pub fn root(&self) -> &LosslessSyntaxNode {
        &self.root
    }

    pub fn tokens(&self) -> &[LosslessToken] {
        &self.tokens
    }

    pub fn write_source(&self, out: &mut String) {
        self.root.write_source(out);
    }

    pub fn source_text(&self) -> String {
        let mut source =
            String::with_capacity(self.tokens.iter().map(|token| token.text.len()).sum());
        self.write_source(&mut source);
        source
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct LosslessSyntaxNode {
    kind: LosslessNodeKind,
    span: Span,
    children: Vec<LosslessSyntaxElement>,
}

impl LosslessSyntaxNode {
    pub fn new(kind: LosslessNodeKind, span: Span, children: Vec<LosslessSyntaxElement>) -> Self {
        Self {
            kind,
            span,
            children,
        }
    }

    pub fn kind(&self) -> &LosslessNodeKind {
        &self.kind
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn children(&self) -> &[LosslessSyntaxElement] {
        &self.children
    }

    fn write_source(&self, out: &mut String) {
        for child in &self.children {
            child.write_source(out);
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessSyntaxElement {
    Node(LosslessSyntaxNode),
    Token(LosslessToken),
}

impl LosslessSyntaxElement {
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LosslessNodeKind {
    File,
    Item(LosslessItemKind),
    Trivia,
}

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
    /// Unknown or unsupported input.
    Unknown,
}
