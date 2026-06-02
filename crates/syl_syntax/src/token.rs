use syl_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenKind {
    /// Identifier, such as `foo` or `my_value`.
    Ident(String),
    /// Unsigned integer literal.
    Int(u64),
    /// String literal without surrounding quotes.
    Str(String),
    /// Boolean literal.
    Bool(bool),
    /// `@`.
    At,
    /// `use`.
    KwUse,
    /// `const`.
    KwConst,
    /// `fn`.
    KwFn,
    /// `let`.
    KwLet,
    /// `return`.
    KwReturn,
    /// `this`.
    KwThis,
    /// `var`.
    KwVar,
    /// `for`.
    KwFor,
    /// `while`.
    KwWhile,
    /// `if`.
    KwIf,
    /// `else`.
    KwElse,
    /// `match`.
    KwMatch,
    /// `select`.
    KwSelect,
    /// `priority`.
    KwPriority,
    /// `unique`.
    KwUnique,
    /// `enum`.
    KwEnum,
    /// `struct`.
    KwStruct,
    /// `bundle`.
    KwBundle,
    /// `interface`.
    KwInterface,
    /// `view`.
    KwView,
    /// `map`.
    KwMap,
    /// `cell`.
    KwCell,
    /// `extern`.
    KwExtern,
    /// `inplace`.
    KwInplace,
    /// `signal`.
    KwSignal,
    /// `reg`.
    KwReg,
    /// `place`.
    KwPlace,
    /// `next`.
    KwNext,
    /// `in`.
    KwIn,
    /// `inout`.
    KwInOut,
    /// `out`.
    KwOut,
    /// `and`.
    KwAnd,
    /// `or`.
    KwOr,
    /// `not`.
    KwNot,
    /// `xor`.
    KwXor,
    /// `eq`.
    KwEqWord,
    /// `+`.
    Plus,
    /// `-`.
    Minus,
    /// `*`.
    Star,
    /// `/`.
    Slash,
    /// `%`.
    Percent,
    /// `=`.
    Eq,
    /// `==`.
    EqEq,
    /// `!`.
    Bang,
    /// `!=`.
    BangEq,
    /// `<`.
    Lt,
    /// `<=`.
    LtEq,
    /// `<<`.
    LtLt,
    /// `>`.
    Gt,
    /// `>=`.
    GtEq,
    /// `&&`.
    AndAnd,
    /// `||`.
    OrOr,
    /// `.`.
    Dot,
    /// `..`.
    DotDot,
    /// `,`.
    Comma,
    /// `:`.
    Colon,
    /// `:=`.
    ColonEq,
    /// `;`.
    Semi,
    /// `->`.
    Arrow,
    /// `=>`.
    EqGt,
    /// `(`.
    LParen,
    /// `)`.
    RParen,
    /// `{`.
    LBrace,
    /// `}`.
    RBrace,
    /// `[`.
    LBracket,
    /// `]`.
    RBracket,
}

/// A single lexed token with its kind and source span.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    /// Creates a new token with the given kind and source span.
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
