use crate::lossless::LosslessItemKind;
use derive_builder::Builder;
use strum_macros::IntoStaticStr;
use syl_span::{SourceId, Span};

mod span;

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AstFile {
    pub source_id: SourceId,
    pub doc: Option<String>,
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Item {
    Error(ErrorItem),
    Use(UseItem),
    Const(ConstItem),
    Fn(FnItem),
    Enum(EnumItem),
    Bundle(BundleItem),
    Interface(InterfaceItem),
    Map(MapItem),
    Cell(CallableItem),
    ExternCell(ExternCellItem),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Self::Error(item) => item.span,
            Self::Use(item) => item.span,
            Self::Const(item) => item.span,
            Self::Fn(item) => item.span,
            Self::Enum(item) => item.span,
            Self::Bundle(item) => item.span,
            Self::Interface(item) => item.span,
            Self::Map(item) => item.span,
            Self::Cell(item) => item.span,
            Self::ExternCell(item) => item.span,
        }
    }

    pub fn lossless_kind(&self) -> LosslessItemKind {
        match self {
            Self::Error(_) => LosslessItemKind::Error,
            Self::Use(_) => LosslessItemKind::Use,
            Self::Const(_) => LosslessItemKind::Const,
            Self::Fn(_) => LosslessItemKind::Fn,
            Self::Enum(_) => LosslessItemKind::Enum,
            Self::Bundle(_) => LosslessItemKind::Bundle,
            Self::Interface(_) => LosslessItemKind::Interface,
            Self::Map(_) => LosslessItemKind::Map,
            Self::Cell(_) => LosslessItemKind::Cell,
            Self::ExternCell(_) => LosslessItemKind::ExternCell,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ErrorItem {
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UseItem {
    pub doc: Option<String>,
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConstItem {
    pub doc: Option<String>,
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct FnItem {
    #[builder(default)]
    pub doc: Option<String>,
    pub name: String,
    #[builder(default)]
    pub params: Vec<Param>,
    #[builder(default)]
    pub ret_ty: Option<TypeExpr>,
    pub body: Block,
    #[builder(default)]
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EnumItem {
    pub doc: Option<String>,
    pub name: String,
    pub width: Option<TypeExpr>,
    pub layout: EnumLayout,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EnumVariant {
    pub doc: Option<String>,
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum EnumLayout {
    #[strum(serialize = "ordinal")]
    Ordinal,
    #[strum(serialize = "flags")]
    Flags,
    #[strum(serialize = "onehot")]
    OneHot,
}

#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct BundleItem {
    #[builder(default)]
    pub doc: Option<String>,
    pub name: String,
    #[builder(default)]
    pub generics: Vec<GenericParam>,
    #[builder(default)]
    pub fields: Vec<FieldDecl>,
    #[builder(default)]
    pub attrs: Vec<Attribute>,
    #[builder(default)]
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct InterfaceItem {
    #[builder(default)]
    pub doc: Option<String>,
    pub name: String,
    #[builder(default)]
    pub generics: Vec<GenericParam>,
    #[builder(default)]
    pub fields: Vec<FieldDecl>,
    #[builder(default)]
    pub views: Vec<ViewDecl>,
    #[builder(default)]
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct MapItem {
    #[builder(default)]
    pub doc: Option<String>,
    pub name: String,
    #[builder(default)]
    pub generics: Vec<GenericParam>,
    #[builder(default)]
    pub params: Vec<Param>,
    #[builder(default)]
    pub ret_ty: Option<TypeExpr>,
    pub body: Expr,
    #[builder(default)]
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct CallableItem {
    #[builder(default)]
    pub doc: Option<String>,
    pub name: String,
    #[builder(default)]
    pub generics: Vec<GenericParam>,
    #[builder(default)]
    pub params: Vec<Param>,
    #[builder(default)]
    pub ports: Vec<PortDecl>,
    #[builder(default)]
    pub result: Option<ResultBinding>,
    pub body: Block,
    #[builder(default)]
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct ExternCellItem {
    #[builder(default)]
    pub doc: Option<String>,
    pub name: String,
    #[builder(default)]
    pub generics: Vec<GenericParam>,
    #[builder(default)]
    pub params: Vec<Param>,
    #[builder(default)]
    pub ports: Vec<PortDecl>,
    #[builder(default)]
    pub result: Option<ResultBinding>,
    #[builder(default)]
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResultBinding {
    pub doc: Option<String>,
    pub name: String,
    pub ty: TypeExpr,
    pub drive: DriveCapability,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PortDecl {
    pub doc: Option<String>,
    pub name: String,
    pub dir: ParamDirection,
    pub ty: TypeExpr,
    pub drive: DriveCapability,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum DriveCapability {
    #[strum(serialize = "ReadOnly")]
    ReadOnly,
    #[strum(serialize = "ReadWrite")]
    ReadWrite,
    #[strum(serialize = "WriteOnly")]
    WriteOnly,
}

impl DriveCapability {
    pub fn can_read(&self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    pub fn can_write(&self) -> bool {
        matches!(self, Self::ReadWrite | Self::WriteOnly)
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Param {
    pub doc: Option<String>,
    pub name: String,
    pub dir: Option<ParamDirection>,
    pub ty: TypeExpr,
    pub role: ParamRole,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamRole {
    Ordinary,
    Receiver,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum ParamDirection {
    #[strum(serialize = "in")]
    In,
    #[strum(serialize = "inout")]
    InOut,
    #[strum(serialize = "out")]
    Out,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct GenericParam {
    pub doc: Option<String>,
    pub name: String,
    pub kind: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FieldDecl {
    pub doc: Option<String>,
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Attribute {
    pub doc: Option<String>,
    pub name: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ViewDecl {
    pub name: String,
    pub fields: Vec<ViewField>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ViewField {
    pub doc: Option<String>,
    pub dir: ViewDirection,
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum ViewDirection {
    #[strum(serialize = "in")]
    In,
    #[strum(serialize = "inout")]
    InOut,
    #[strum(serialize = "out")]
    Out,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Stmt {
    Error {
        span: Span,
    },
    Const {
        name: String,
        ty: Option<TypeExpr>,
        value: Expr,
        span: Span,
    },
    Let {
        name: String,
        ty: Option<TypeExpr>,
        value: Option<Expr>,
        span: Span,
    },
    Var {
        name: String,
        ty: Option<TypeExpr>,
        value: Option<Expr>,
        span: Span,
    },
    Signal {
        name: String,
        ty: Option<TypeExpr>,
        value: Option<Expr>,
        span: Span,
    },
    Reg {
        name: String,
        ty: Option<TypeExpr>,
        reset: Option<RegReset>,
        span: Span,
    },
    Assign {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Drive {
        target: Expr,
        value: Expr,
        span: Span,
    },
    Next {
        name: String,
        value: Expr,
        span: Span,
    },
    While {
        cond: Expr,
        body: Block,
        span: Span,
    },
    ElabIf {
        cond: Expr,
        then_block: Block,
        else_block: Option<Block>,
        span: Span,
    },
    ElabFor {
        name: String,
        range: Expr,
        body: Block,
        span: Span,
    },
    Expr(Expr),
    Return(Option<Expr>, Span),
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct RegReset {
    pub domain: Option<Expr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Expr {
    Ident(String, Span),
    Int(u64, Span),
    Str(String, Span),
    Bool(bool, Span),
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expr>,
        right: Box<Expr>,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<CallArg>,
        span: Span,
    },
    GenericApp {
        callee: Box<Expr>,
        args: Vec<TypeExpr>,
        span: Span,
    },
    Aggregate {
        ty: Box<TypeExpr>,
        fields: Vec<NamedExpr>,
        span: Span,
    },
    Field {
        base: Box<Expr>,
        field: String,
        span: Span,
    },
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Group(Box<Expr>, Span),
    Block(Block),
    Match {
        expr: Box<Expr>,
        arms: Vec<MatchArm>,
        span: Span,
    },
    Select {
        mode: SelectMode,
        arms: Vec<SelectArm>,
        span: Span,
    },
    Place {
        callee: Box<Expr>,
        args: Vec<CallArg>,
        inplace: bool,
        span: Span,
    },
    For {
        name: String,
        range: Box<Expr>,
        body: Block,
        span: Span,
    },
    CompileError {
        message: Box<Expr>,
        span: Span,
    },
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NamedExpr {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum SelectMode {
    #[strum(serialize = "priority")]
    Priority,
    #[strum(serialize = "unique")]
    Unique,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SelectArm {
    pub doc: Option<String>,
    pub pattern: Expr,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MatchArm {
    pub doc: Option<String>,
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Pattern {
    Wildcard(Span),
    Ident(String, Span),
    Int(u64, Span),
    Bool(bool, Span),
    Path(Vec<String>, Span),
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TypeExpr {
    Path(Vec<String>, Span),
    Array {
        len: Box<Expr>,
        elem: Box<TypeExpr>,
        span: Span,
    },
    Generic {
        base: Box<TypeExpr>,
        args: Vec<TypeExpr>,
        span: Span,
    },
    ViewSelect {
        base: Box<TypeExpr>,
        view: String,
        span: Span,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum UnaryOp {
    #[strum(serialize = "-")]
    Neg,
    #[strum(serialize = "!")]
    Not,
    #[strum(serialize = "not")]
    NotWord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, IntoStaticStr)]
#[non_exhaustive]
pub enum BinaryOp {
    #[strum(serialize = "||")]
    OrOr,
    #[strum(serialize = "&&")]
    AndAnd,
    #[strum(serialize = "==")]
    EqEq,
    #[strum(serialize = "!=")]
    NotEq,
    #[strum(serialize = "<")]
    Lt,
    #[strum(serialize = "<=")]
    LtEq,
    #[strum(serialize = ">")]
    Gt,
    #[strum(serialize = ">=")]
    GtEq,
    #[strum(serialize = "+")]
    Add,
    #[strum(serialize = "-")]
    Sub,
    #[strum(serialize = "*")]
    Mul,
    #[strum(serialize = "/")]
    Div,
    #[strum(serialize = "%")]
    Rem,
    #[strum(serialize = "<<")]
    Shl,
    #[strum(serialize = ".")]
    Field,
    #[strum(serialize = "and")]
    AndWord,
    #[strum(serialize = "or")]
    OrWord,
    #[strum(serialize = "xor")]
    XorWord,
    #[strum(serialize = "eq")]
    EqWord,
}
