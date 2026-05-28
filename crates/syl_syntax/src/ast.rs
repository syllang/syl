use crate::lossless::LosslessItemKind;
use derive_builder::Builder;
use strum_macros::IntoStaticStr;
use syl_span::{SourceId, Span};

// `span` only holds span accessors for AST node types. Keep it separate from
// the type definitions below to avoid mixing data layout with derived helpers.
mod span;

/// Top-level parsed source file.
///
/// Models one `.syl` source file as an ordered list of top-level items
/// (`use`, `const`, `fn`, `enum`, `bundle`, `interface`, `map`, `cell`,
/// `extern cell`) and an optional module-level doc comment.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct AstFile {
    pub source_id: SourceId,
    pub doc: Option<String>,
    pub items: Vec<Item>,
}

/// Any top-level item that can appear in a `.syl` source file.
///
/// Each variant wraps the corresponding item struct. Dispatch on this
/// when you need to handle all possible declarations generically.
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
    /// Returns the source span of the underlying item.
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

    /// Returns the lossless syntax tree item kind for this declaration.
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

/// Placeholder item produced when the parser encounters a syntax error.
///
/// Keeps the span of the malformed region so downstream passes can still
/// report sensible error locations.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ErrorItem {
    pub span: Span,
}

/// A `use` declaration that imports symbols from another module.
///
/// `path` is a segmented identifier (e.g. `["std", "logic"]` for `use std::logic`).
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct UseItem {
    pub doc: Option<String>,
    pub path: Vec<String>,
    pub span: Span,
}

/// A `const` declaration that binds a name to a compile-time evaluable expression.
///
/// The type can be inferred when `ty` is `None`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ConstItem {
    pub doc: Option<String>,
    pub name: String,
    pub ty: Option<TypeExpr>,
    pub value: Expr,
    pub span: Span,
}

/// A function declaration — a named, reusable block with parameters and an optional return type.
///
/// Functions are elaboration-time combinators that produce hardware.
/// The body is a `Block` containing statements and an optional tail expression.
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

/// An `enum` declaration — a named set of symbolic states backed by a hardware-encoded value.
///
/// `width` controls the bit-width of the encoding; `layout` selects the
/// encoding scheme (ordinal, flags, or one-hot).
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

/// A single variant within an `enum` declaration.
///
/// `value` is the explicit encoding expression (e.g. `1 << 2`).
/// When `None`, the layout strategy assigns an implicit value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct EnumVariant {
    pub doc: Option<String>,
    pub name: String,
    pub value: Option<Expr>,
    pub span: Span,
}

/// Encoding scheme for an enum's hardware representation.
///
/// - `Ordinal`: sequential binary encoding (0, 1, 2, …).
/// - `Flags`: bit-field where each variant is a single bit.
/// - `OneHot`: exactly one bit set at any time.
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

/// A `bundle` declaration — a named group of named fields (struct-like).
///
/// Bundles are the primary way to compose related signals into a single
/// named type, analogous to a `struct` in software languages.
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

/// An `interface` declaration — a bundle with multiple *views* for directional access.
///
/// Interfaces extend bundles by associating each field subset with a named
/// view that specifies which fields are readable or writable from that side.
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

/// A `map` declaration — a pure function whose body is a single expression.
///
/// Maps are elaboration-time combinators that must be side-effect-free.
/// Unlike `fn`, a map's body is exactly one expression (no statements).
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

/// A `cell` declaration — a hardware module with parameter inputs and port-based IO.
///
/// Cells are the central building block of hardware design in Syl: they
/// have elaboration-time parameters (`params`) and hardware ports (`ports`)
/// that become wires in the generated circuit.
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

/// An `extern cell` declaration — a hardware module with no body (imported).
///
/// Extern cells describe the interface of a module defined externally,
/// typically in SystemVerilog or another source. The compiler uses this
/// to type-check connections without seeing the implementation.
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

/// The named result of a cell's combined output port.
///
/// In `cell foo -> (result: T)`, the `ResultBinding` captures the name,
/// type, and drive capability of the result signal produced by the cell.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ResultBinding {
    pub doc: Option<String>,
    pub name: String,
    pub ty: TypeExpr,
    pub drive: DriveCapability,
    pub span: Span,
}

/// A single port declaration on a cell's interface.
///
/// Ports are the hardware-level IO of a module: direction (`in`, `inout`, `out`),
/// type, and drive capability describe how the port connects to other modules.
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

/// How a signal is allowed to be driven on a particular connection.
///
/// `ReadOnly` — the receiver can only observe the value.
/// `ReadWrite` — the receiver can both observe and drive.
/// `WriteOnly` — the receiver can only drive (e.g. an output wire).
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
    /// Returns `true` if this capability permits reading the signal value.
    pub fn can_read(&self) -> bool {
        matches!(self, Self::ReadOnly | Self::ReadWrite)
    }

    /// Returns `true` if this capability permits driving (writing) the signal.
    pub fn can_write(&self) -> bool {
        matches!(self, Self::ReadWrite | Self::WriteOnly)
    }
}

/// A single parameter on a function, map, or cell declaration.
///
/// Parameters are elaboration-time values. The `dir` field is `Some` only
/// when the parameter has an explicit direction annotation (e.g. `in`).
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

/// Whether a parameter is an ordinary value or the implicit receiver (`this`).
///
/// `Receiver` marks the special `this` binding that refers to the enclosing
/// bundle/interface instance in extension methods.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ParamRole {
    Ordinary,
    Receiver,
}

/// Direction qualifier for a parameter or port.
///
/// Mirrors SystemVerilog port directions.
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

/// A type or const generic parameter on a declaration.
///
/// `kind` constrains the parameter to a specific type (e.g. `Int<4>`).
/// `default` provides an optional default expression when omitted at the call site.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct GenericParam {
    pub doc: Option<String>,
    pub name: String,
    pub kind: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub span: Span,
}

/// A named field in a bundle, interface, or hardware struct type.
///
/// Analogous to a struct field in software — it pairs a name with a type.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct FieldDecl {
    pub doc: Option<String>,
    pub name: String,
    pub ty: TypeExpr,
    pub span: Span,
}

/// An annotation attached to a bundle or interface declaration.
///
/// Attributes provide metadata (e.g. register inference hints) and have
/// a name plus optional argument expressions.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Attribute {
    pub doc: Option<String>,
    pub name: String,
    pub args: Vec<Expr>,
    pub span: Span,
}

/// A named *view* inside an interface — describes which fields are visible
/// from a particular connection side and with what directionality.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ViewDecl {
    pub name: String,
    pub fields: Vec<ViewField>,
    pub span: Span,
}

/// A single field within a view declaration, annotated with its visible direction.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct ViewField {
    pub doc: Option<String>,
    pub dir: ViewDirection,
    pub name: String,
    pub span: Span,
}

/// Direction qualifier for a view field's access from that view's connection side.
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

/// A braced block of statements with an optional tail expression.
///
/// Every statement list is terminated by `;`. Any expression that
/// appears without a trailing `;` becomes the block's tail value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub tail: Option<Box<Expr>>,
    pub span: Span,
}

/// A single statement within a block body.
///
/// Covers control flow (`while`, `if`/`else`, `for`), variable-like
/// declarations (`let`, `var`, `signal`, `reg`, `const`), assignments,
/// drive statements, and plain expressions evaluated for side effects.
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

/// Reset specification for a register declaration.
///
/// `domain` is the optional reset clock/domain expression. `value` is
/// the value the register assumes when reset is asserted.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct RegReset {
    pub domain: Option<Expr>,
    pub value: Expr,
    pub span: Span,
}

/// Any expression in the Syl language.
///
/// Expressions cover literals, operators, function calls, type application,
/// field/index access, match/select, and inline blocks.
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

/// A named (key-value) expression, e.g. `field = value` in an aggregate literal.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct NamedExpr {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

/// A single argument in a function or cell call, optionally named.
///
/// Named arguments (`name: expr`) allow out-of-order parameter binding.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct CallArg {
    pub name: Option<String>,
    pub value: Expr,
    pub span: Span,
}

/// Whether a `select` expression uses priority or unique evaluation semantics.
///
/// `Priority` — evaluates arms in order, first match wins.
/// `Unique` — exactly one arm must match (parallel, checked by semantics).
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum SelectMode {
    #[strum(serialize = "priority")]
    Priority,
    #[strum(serialize = "unique")]
    Unique,
}

/// A single arm in a `select` expression: pattern expression → result value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SelectArm {
    pub doc: Option<String>,
    pub pattern: Expr,
    pub value: Expr,
    pub span: Span,
}

/// A single arm in a `match` expression: structured pattern → result value.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MatchArm {
    pub doc: Option<String>,
    pub pattern: Pattern,
    pub value: Expr,
    pub span: Span,
}

/// A pattern in a `match` arm — describes the shape of values to match.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum Pattern {
    Wildcard(Span),
    Ident(String, Span),
    Int(u64, Span),
    Bool(bool, Span),
    Path(Vec<String>, Span),
}

/// A type expression, referencing a named type, array, generic, or view selection.
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

/// A unary operator applied to a single expression.
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

/// A binary operator combining two sub-expressions.
///
/// Includes comparison (`==`, `<`, …), arithmetic (`+`, `-`, …),
/// bitwise (`and`, `or`, `xor`), and equality-word (`eq`) operators.
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
    #[strum(serialize = "and")]
    AndWord,
    #[strum(serialize = "or")]
    OrWord,
    #[strum(serialize = "xor")]
    XorWord,
    #[strum(serialize = "eq")]
    EqWord,
}
