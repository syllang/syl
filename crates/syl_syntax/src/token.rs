use syl_span::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TokenKind {
    Ident(String),
    Int(u64),
    Str(String),
    Bool(bool),
    At,
    KwUse,
    KwConst,
    KwFn,
    KwLet,
    KwReturn,
    KwThis,
    KwVar,
    KwFor,
    KwWhile,
    KwIf,
    KwElse,
    KwMatch,
    KwSelect,
    KwPriority,
    KwUnique,
    KwEnum,
    KwBundle,
    KwInterface,
    KwView,
    KwMap,
    KwCell,
    KwModule,
    KwExtern,
    KwSignal,
    KwReg,
    KwPlace,
    KwNext,
    KwIn,
    KwInOut,
    KwOut,
    KwAnd,
    KwOr,
    KwNot,
    KwXor,
    KwEqWord,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    EqEq,
    Bang,
    BangEq,
    Lt,
    LtEq,
    LtLt,
    Gt,
    GtEq,
    AndAnd,
    OrOr,
    Dot,
    DotDot,
    Comma,
    Colon,
    ColonEq,
    Semi,
    Arrow,
    EqGt,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }
}
