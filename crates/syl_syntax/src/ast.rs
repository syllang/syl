use crate::lossless::LosslessItemKind;
use syl_span::Span;

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AstFile {
    pub items: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Item {
    Error(ErrorItem),
    Package(PackageItem),
    Use(UseItem),
    Const(ConstItem),
    Fn(FnItem),
    Enum(EnumItem),
    Bundle(BundleItem),
    Interface(InterfaceItem),
    Map(MapItem),
    Cell(CallableItem),
    Module(CallableItem),
    ExternModule(ExternModuleItem),
}

impl Item {
    pub fn span(&self) -> Span {
        match self {
            Self::Error(item) => item.span,
            Self::Package(item) => item.span,
            Self::Use(item) => item.span,
            Self::Const(item) => item.span,
            Self::Fn(item) => item.span,
            Self::Enum(item) => item.span,
            Self::Bundle(item) => item.span,
            Self::Interface(item) => item.span,
            Self::Map(item) => item.span,
            Self::Cell(item) => item.span,
            Self::Module(item) => item.span,
            Self::ExternModule(item) => item.span,
        }
    }

    pub fn lossless_kind(&self) -> LosslessItemKind {
        match self {
            Self::Error(_) => LosslessItemKind::Error,
            Self::Package(_) => LosslessItemKind::Package,
            Self::Use(_) => LosslessItemKind::Use,
            Self::Const(_) => LosslessItemKind::Const,
            Self::Fn(_) => LosslessItemKind::Fn,
            Self::Enum(_) => LosslessItemKind::Enum,
            Self::Bundle(_) => LosslessItemKind::Bundle,
            Self::Interface(_) => LosslessItemKind::Interface,
            Self::Map(_) => LosslessItemKind::Map,
            Self::Cell(_) => LosslessItemKind::Cell,
            Self::Module(_) => LosslessItemKind::Module,
            Self::ExternModule(_) => LosslessItemKind::ExternModule,
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
pub struct PackageItem {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UseItem {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConstItem {
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FnItem {
    pub name: String,
    pub params: Vec<Param>,
    pub ret_ty: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EnumItem {
    pub name: String,
    pub variants: Vec<EnumVariant>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EnumVariant {
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct BundleItem {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<FieldDecl>,
    pub attrs: Vec<Attribute>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct InterfaceItem {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<FieldDecl>,
    pub views: Vec<ViewDecl>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MapItem {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ret_ty: Option<TypeExpr>,
    pub body: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CallableItem {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ports: Vec<PortDecl>,
    pub result: Option<ResultBinding>,
    pub body: Block,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ExternModuleItem {
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub ports: Vec<PortDecl>,
    pub result: Option<ResultBinding>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResultBinding {
    pub name: String,
    pub ty: TypeExpr,
    pub drive: DriveCapability,
    pub span: Span,
}

impl ResultBinding {
    pub fn is_drivable(&self) -> bool {
        self.drive.can_write()
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct PortDecl {
    pub name: String,
    pub dir: ParamDirection,
    pub ty: TypeExpr,
    pub drive: DriveCapability,
    pub span: Span,
}

impl PortDecl {
    pub fn is_in(&self) -> bool {
        self.dir.is_in()
    }

    pub fn is_out(&self) -> bool {
        self.dir.is_out()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DriveCapability {
    ReadOnly,
    ReadWrite,
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
    pub name: String,
    pub dir: Option<ParamDirection>,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamDirection {
    In,
    InOut,
    Out,
}

impl ParamDirection {
    pub fn is_in(&self) -> bool {
        match self {
            Self::In => true,
            Self::InOut => true,
            Self::Out => false,
        }
    }

    pub fn is_out(&self) -> bool {
        match self {
            Self::In => false,
            Self::InOut => true,
            Self::Out => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct GenericParam {
    pub name: String,
    pub kind: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FieldDecl {
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Attribute {
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
    pub dir: ViewDirection,
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ViewDirection {
    In,
    InOut,
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

impl Expr {
    pub fn span(&self) -> Span {
        match self {
            Expr::Ident(_, span)
            | Expr::Int(_, span)
            | Expr::Str(_, span)
            | Expr::Bool(_, span)
            | Expr::Group(_, span) => *span,
            Expr::Unary { span, .. }
            | Expr::Binary { span, .. }
            | Expr::Call { span, .. }
            | Expr::GenericApp { span, .. }
            | Expr::Aggregate { span, .. }
            | Expr::Field { span, .. }
            | Expr::Index { span, .. }
            | Expr::Match { span, .. }
            | Expr::Select { span, .. }
            | Expr::Place { span, .. }
            | Expr::For { span, .. }
            | Expr::CompileError { span, .. }
            | Expr::Range { span, .. } => *span,
            Expr::Block(block) => block.span,
        }
    }
}

impl Stmt {
    pub fn span(&self) -> Span {
        match self {
            Self::Error { span }
            | Self::Const { span, .. }
            | Self::Let { span, .. }
            | Self::Var { span, .. }
            | Self::Signal { span, .. }
            | Self::Reg { span, .. }
            | Self::Next { span, .. }
            | Self::While { span, .. }
            | Self::ElabIf { span, .. }
            | Self::ElabFor { span, .. } => *span,
            Self::Expr(expr) => expr.span(),
            Self::Return(_, span) => *span,
        }
    }
}

impl TypeExpr {
    pub fn span(&self) -> Span {
        match self {
            Self::Path(_, span)
            | Self::Array { span, .. }
            | Self::Generic { span, .. }
            | Self::ViewSelect { span, .. } => *span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SelectMode {
    Priority,
    Unique,
}

impl SelectMode {
    pub fn is_unique(&self) -> bool {
        match self {
            Self::Priority => false,
            Self::Unique => true,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SelectArm {
    pub pattern: Expr,
    pub value: Expr,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MatchArm {
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

impl Pattern {
    pub fn span(&self) -> Span {
        match self {
            Self::Wildcard(span)
            | Self::Ident(_, span)
            | Self::Int(_, span)
            | Self::Bool(_, span)
            | Self::Path(_, span) => *span,
        }
    }
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum UnaryOp {
    Neg,
    Not,
    NotWord,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub enum BinaryOp {
    Assign,
    OrOr,
    AndAnd,
    EqEq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Shl,
    Field,
    AndWord,
    OrWord,
    XorWord,
    EqWord,
}

impl From<UnaryOp> for &'static str {
    fn from(value: UnaryOp) -> Self {
        match value {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
            UnaryOp::NotWord => "not",
        }
    }
}

impl From<BinaryOp> for &'static str {
    fn from(value: BinaryOp) -> Self {
        match value {
            BinaryOp::Assign => "=",
            BinaryOp::OrOr => "||",
            BinaryOp::AndAnd => "&&",
            BinaryOp::EqEq => "==",
            BinaryOp::NotEq => "!=",
            BinaryOp::Lt => "<",
            BinaryOp::LtEq => "<=",
            BinaryOp::Gt => ">",
            BinaryOp::GtEq => ">=",
            BinaryOp::Add => "+",
            BinaryOp::Sub => "-",
            BinaryOp::Mul => "*",
            BinaryOp::Div => "/",
            BinaryOp::Rem => "%",
            BinaryOp::Shl => "<<",
            BinaryOp::Field => ".",
            BinaryOp::AndWord => "and",
            BinaryOp::OrWord => "or",
            BinaryOp::XorWord => "xor",
            BinaryOp::EqWord => "eq",
        }
    }
}
