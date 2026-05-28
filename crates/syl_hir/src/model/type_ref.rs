use std::collections::HashMap;
use strum_macros::IntoStaticStr;
use syl_span::Span;
use syl_syntax::{BinaryOp, Expr, Pattern, SelectMode, TypeExpr, UnaryOp};

/// Unary operator in the MIR (mid-level IR) type system.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum MirUnaryOp {
    #[strum(serialize = "-")]
    Neg,
    #[strum(serialize = "!")]
    Not,
    #[strum(serialize = "not")]
    NotWord,
    #[strum(serialize = "?")]
    Unsupported,
}

impl From<UnaryOp> for MirUnaryOp {
    fn from(op: UnaryOp) -> Self {
        match op {
            UnaryOp::Neg => Self::Neg,
            UnaryOp::Not => Self::Not,
            UnaryOp::NotWord => Self::NotWord,
            _ => Self::Unsupported,
        }
    }
}

/// Binary operator in the MIR (mid-level IR) type system.
///
/// Includes assignment, comparison, arithmetic, bitwise, and word operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum MirBinaryOp {
    #[strum(serialize = "=")]
    Assign,
    #[strum(serialize = "||")]
    OrOr,
    #[strum(serialize = "&&")]
    AndAnd,
    #[strum(serialize = "==")]
    Eq,
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
    BitAnd,
    #[strum(serialize = "or")]
    BitOr,
    #[strum(serialize = "xor")]
    BitXor,
    #[strum(serialize = "?")]
    Unsupported,
}

impl From<BinaryOp> for MirBinaryOp {
    fn from(op: BinaryOp) -> Self {
        match op {
            BinaryOp::OrOr => Self::OrOr,
            BinaryOp::AndAnd => Self::AndAnd,
            BinaryOp::EqEq | BinaryOp::EqWord => Self::Eq,
            BinaryOp::NotEq => Self::NotEq,
            BinaryOp::Lt => Self::Lt,
            BinaryOp::LtEq => Self::LtEq,
            BinaryOp::Gt => Self::Gt,
            BinaryOp::GtEq => Self::GtEq,
            BinaryOp::Add => Self::Add,
            BinaryOp::Sub => Self::Sub,
            BinaryOp::Mul => Self::Mul,
            BinaryOp::Div => Self::Div,
            BinaryOp::Rem => Self::Rem,
            BinaryOp::Shl => Self::Shl,
            BinaryOp::AndWord => Self::BitAnd,
            BinaryOp::OrWord => Self::BitOr,
            BinaryOp::XorWord => Self::BitXor,
            _ => Self::Unsupported,
        }
    }
}

/// Select evaluation mode in the MIR: priority or unique.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum MirSelectMode {
    #[strum(serialize = "priority")]
    Priority,
    #[strum(serialize = "unique")]
    Unique,
}

impl From<SelectMode> for MirSelectMode {
    fn from(mode: SelectMode) -> Self {
        match mode {
            SelectMode::Priority => Self::Priority,
            SelectMode::Unique => Self::Unique,
            _ => Self::Priority,
        }
    }
}

/// A pattern in the MIR, used in match arms.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MirPattern {
    Wildcard(Span),
    Ident(String, Span),
    Int(u64, Span),
    Bool(bool, Span),
    Path(Vec<String>, Span),
    Unsupported(Span),
}

impl From<&Pattern> for MirPattern {
    fn from(pattern: &Pattern) -> Self {
        match pattern {
            Pattern::Wildcard(span) => Self::Wildcard(*span),
            Pattern::Ident(name, span) => Self::Ident(name.clone(), *span),
            Pattern::Int(value, span) => Self::Int(*value, *span),
            Pattern::Bool(value, span) => Self::Bool(*value, *span),
            Pattern::Path(path, span) => Self::Path(path.clone(), *span),
            _ => Self::Unsupported(pattern.span()),
        }
    }
}

/// A type reference in the MIR — a resolved type expression.
///
/// `MirTypeRef` is a lightweight handle that wraps `MirTypeKind` with a
/// source span. It supports path lookup, generic instantiation, array
/// decomposition, and view selection.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MirTypeRef {
    kind: MirTypeKind,
    span: Span,
}

impl MirTypeRef {
    /// Creates a path-based type reference (e.g. `UInt`).
    pub fn path_type(path: Vec<String>, span: Span) -> Self {
        Self {
            kind: MirTypeKind::Path(path),
            span,
        }
    }

    /// Creates an array type reference: `[len]elem`.
    pub fn array_type(len: MirConstExpr, elem: MirTypeRef, span: Span) -> Self {
        Self {
            kind: MirTypeKind::Array {
                len,
                elem: Box::new(elem),
            },
            span,
        }
    }

    /// Creates a generic type reference: `Base<Arg1, Arg2>`.
    pub fn generic_type(base: MirTypeRef, args: Vec<MirTypeRef>, span: Span) -> Self {
        Self {
            kind: MirTypeKind::Generic {
                base: Box::new(base),
                args,
            },
            span,
        }
    }

    /// Creates a view-select type reference: `Base::View`.
    pub fn view_select_type(base: MirTypeRef, view: String, span: Span) -> Self {
        Self {
            kind: MirTypeKind::ViewSelect {
                base: Box::new(base),
                view,
            },
            span,
        }
    }

    /// Creates a placeholder for unsupported type expressions.
    pub fn unsupported(span: Span) -> Self {
        Self {
            kind: MirTypeKind::Unsupported,
            span,
        }
    }

    /// Returns the source span of this type reference.
    pub fn span(&self) -> Span {
        self.span
    }

    /// If this is a path type, returns the path segments.
    pub fn path(&self) -> Option<&[String]> {
        match &self.kind {
            MirTypeKind::Path(path) => Some(path),
            _ => None,
        }
    }

    /// Returns the last segment of a path type (the type name).
    pub fn path_name(&self) -> Option<&str> {
        self.path()?.last().map(String::as_str)
    }

    /// Returns the human-readable name of this type, following through
    /// generics, view-selects, and arrays to find a leaf name.
    pub fn type_name(&self) -> Option<&str> {
        match &self.kind {
            MirTypeKind::Path(path) => path.last().map(String::as_str),
            MirTypeKind::Generic { base, .. } | MirTypeKind::ViewSelect { base, .. } => {
                base.type_name()
            }
            MirTypeKind::Array { elem, .. } => elem.type_name(),
            MirTypeKind::Unsupported => None,
        }
    }

    /// Returns the generic type arguments, if this is a generic or view-select type.
    pub fn args(&self) -> Option<&[MirTypeRef]> {
        match &self.kind {
            MirTypeKind::Generic { args, .. } => Some(args),
            MirTypeKind::ViewSelect { base, .. } => base.args(),
            _ => None,
        }
    }

    /// If this is a generic type, returns the base type being parameterized.
    pub fn generic_base(&self) -> Option<&MirTypeRef> {
        match &self.kind {
            MirTypeKind::Generic { base, .. } => Some(base),
            _ => None,
        }
    }

    /// If this is an array type, returns (length, element type).
    pub fn array(&self) -> Option<(&MirConstExpr, &MirTypeRef)> {
        match &self.kind {
            MirTypeKind::Array { len, elem } => Some((len, elem)),
            _ => None,
        }
    }

    /// If this is a view-select type, returns (base type, view name).
    pub fn view_select(&self) -> Option<(&MirTypeRef, &str)> {
        match &self.kind {
            MirTypeKind::ViewSelect { base, view } => Some((base, view)),
            _ => None,
        }
    }

    /// Substitutes type variables using the given replacement map.
    ///
    /// **Critical — single-segment heuristic:** Only `Path` types with exactly
    /// one segment are treated as type variables eligible for substitution.
    /// A path like `["UInt"]` is replaced if `"UInt"` is a key; a path like
    /// `["std", "logic", "UInt"]` is **never** substituted, even if the same
    /// name `"UInt"` exists in the map. This prevents accidental replacement
    /// of fully-qualified type names that happen to match a generic parameter
    /// name.
    ///
    /// ```ignore
    /// let mut map = HashMap::new();
    /// map.insert("T".into(), MirTypeRef::path_type(vec!["UInt".into()], span));
    /// // Path(["T"])       → Path(["UInt"])   — single segment, substituted
    /// // Path(["Foo","T"]) → Path(["Foo","T"]) — multi segment, unchanged
    /// ```
    pub fn subst(&self, replacements: &HashMap<String, MirTypeRef>) -> Self {
        match &self.kind {
            MirTypeKind::Path(path) if path.len() == 1 => replacements
                .get(&path[0])
                .cloned()
                .unwrap_or_else(|| self.clone()),
            MirTypeKind::Path(_) | MirTypeKind::Unsupported => self.clone(),
            MirTypeKind::Array { len, elem } => Self::array_type(
                len.subst_type_vars(replacements),
                elem.subst(replacements),
                self.span,
            ),
            MirTypeKind::Generic { base, args } => Self::generic_type(
                base.subst(replacements),
                args.iter().map(|arg| arg.subst(replacements)).collect(),
                self.span,
            ),
            MirTypeKind::ViewSelect { base, view } => {
                Self::view_select_type(base.subst(replacements), view.clone(), self.span)
            }
        }
    }

    /// Wraps this type as an array of the given length: `[len]self`.
    pub fn with_array_len(self, len: MirConstExpr, span: Span) -> Self {
        Self::array_type(len, self, span)
    }
}

/// The inner kind of a `MirTypeRef`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MirTypeKind {
    Path(Vec<String>),
    Array {
        len: MirConstExpr,
        elem: Box<MirTypeRef>,
    },
    Generic {
        base: Box<MirTypeRef>,
        args: Vec<MirTypeRef>,
    },
    ViewSelect {
        base: Box<MirTypeRef>,
        view: String,
    },
    Unsupported,
}

impl From<&TypeExpr> for MirTypeRef {
    fn from(ty: &TypeExpr) -> Self {
        match ty {
            TypeExpr::Path(path, span) => Self {
                kind: MirTypeKind::Path(path.clone()),
                span: *span,
            },
            TypeExpr::Array { len, elem, span } => Self {
                kind: MirTypeKind::Array {
                    len: MirConstExpr::from(len.as_ref()),
                    elem: Box::new(Self::from(elem.as_ref())),
                },
                span: *span,
            },
            TypeExpr::Generic { base, args, span } => Self {
                kind: MirTypeKind::Generic {
                    base: Box::new(Self::from(base.as_ref())),
                    args: args.iter().map(Self::from).collect(),
                },
                span: *span,
            },
            TypeExpr::ViewSelect { base, view, span } => Self {
                kind: MirTypeKind::ViewSelect {
                    base: Box::new(Self::from(base.as_ref())),
                    view: view.clone(),
                },
                span: *span,
            },
            _ => Self {
                kind: MirTypeKind::Unsupported,
                span: ty.span(),
            },
        }
    }
}

/// A constant expression in the MIR: literal, identifier, or operation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MirConstExpr {
    kind: MirConstExprKind,
    span: Span,
}

impl MirConstExpr {
    /// Creates a constant reference to a name.
    pub fn ident_expr(name: String, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Ident(name),
            span,
        }
    }

    /// Creates a natural number constant.
    pub fn nat(value: u64, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Nat(value),
            span,
        }
    }

    /// Creates a boolean constant.
    pub fn bool_value_expr(value: bool, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Bool(value),
            span,
        }
    }

    /// Creates a unary operation constant.
    pub fn unary_expr(op: MirUnaryOp, expr: MirConstExpr, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Unary {
                op,
                expr: Box::new(expr),
            },
            span,
        }
    }

    /// Creates a binary operation constant.
    pub fn binary_expr(
        op: MirBinaryOp,
        left: MirConstExpr,
        right: MirConstExpr,
        span: Span,
    ) -> Self {
        Self {
            kind: MirConstExprKind::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            },
            span,
        }
    }

    /// Returns the source span of this constant expression.
    pub fn span(&self) -> Span {
        self.span
    }

    /// If this is an identifier constant, returns the name.
    pub fn ident(&self) -> Option<&str> {
        match &self.kind {
            MirConstExprKind::Ident(name) => Some(name),
            _ => None,
        }
    }

    /// If this is a natural number constant, returns its value.
    pub fn nat_value(&self) -> Option<u64> {
        match &self.kind {
            MirConstExprKind::Nat(value) => Some(*value),
            _ => None,
        }
    }

    /// If this is a boolean constant, returns its value.
    pub fn bool_value(&self) -> Option<bool> {
        match &self.kind {
            MirConstExprKind::Bool(value) => Some(*value),
            _ => None,
        }
    }

    /// If this is a unary operation, returns (operator, operand).
    pub fn unary(&self) -> Option<(MirUnaryOp, &MirConstExpr)> {
        match &self.kind {
            MirConstExprKind::Unary { op, expr } => Some((*op, expr)),
            _ => None,
        }
    }

    /// If this is a binary operation, returns (operator, left, right).
    pub fn binary(&self) -> Option<(MirBinaryOp, &MirConstExpr, &MirConstExpr)> {
        match &self.kind {
            MirConstExprKind::Binary { op, left, right } => Some((*op, left, right)),
            _ => None,
        }
    }

    /// Substitute type variables in this const expression.
    ///
    /// Like `MirTypeRef::subst`, but operates on const-level `Ident` nodes.
    /// The replacement is parsed via `from_type_arg` which may silently change
    /// semantics: a replacement `MirTypeRef::path(vec!["8"])` becomes `Nat(8)`,
    /// while `MirTypeRef::path(vec!["foo"])` stays as `Ident("foo")`.
    ///
    /// **Namespace-sensitive:** Multi-segment paths are rejected here rather
    /// than being collapsed to their last segment. That keeps const
    /// substitution from silently discarding namespace context.
    ///
    /// ```ignore
    /// // Replacement {"N" → MirTypeRef::path(vec!["8"])} on Ident("N")
    /// //   → Nat(8)   // string "8" parsed as u64
    /// // Replacement {"N" → MirTypeRef::path(vec!["foo"])} on Ident("N")
    /// //   → Ident("foo")   // stays as identifier
    /// ```
    fn subst_type_vars(&self, replacements: &HashMap<String, MirTypeRef>) -> Self {
        match &self.kind {
            MirConstExprKind::Ident(name) => replacements
                .get(name)
                .and_then(Self::from_type_arg)
                .unwrap_or_else(|| self.clone()),
            MirConstExprKind::Unary { op, expr } => {
                Self::unary_expr(*op, expr.subst_type_vars(replacements), self.span)
            }
            MirConstExprKind::Binary { op, left, right } => Self::binary_expr(
                *op,
                left.subst_type_vars(replacements),
                right.subst_type_vars(replacements),
                self.span,
            ),
            MirConstExprKind::Nat(_)
            | MirConstExprKind::Bool(_)
            | MirConstExprKind::Unsupported => self.clone(),
        }
    }

    /// Try to interpret a `MirTypeRef` (used as a type argument) as a const expression.
    ///
    /// This is a **lossy conversion**: a type path like `["8"]` becomes `Nat(8)`,
    /// `["true"]` becomes `Bool(true)`, and anything else becomes `Ident(name)`.
    ///
    /// Returns `None` if the type is not a single-segment path (for example an
    /// array, generic, or multi-segment path used as a const argument — which
    /// should not happen in well-formed code, but is treated conservatively
    /// here).
    fn from_type_arg(ty: &MirTypeRef) -> Option<Self> {
        let [name] = ty.path()? else {
            return None;
        };
        let name = name.as_str();
        let kind = if let Ok(value) = name.parse::<u64>() {
            MirConstExprKind::Nat(value)
        } else if name == "true" {
            MirConstExprKind::Bool(true)
        } else if name == "false" {
            MirConstExprKind::Bool(false)
        } else {
            MirConstExprKind::Ident(name.to_string())
        };
        Some(Self {
            kind,
            span: ty.span(),
        })
    }
}

#[cfg(test)]
mod tests;

/// The inner kind of a `MirConstExpr`.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MirConstExprKind {
    Ident(String),
    Nat(u64),
    Bool(bool),
    Unary {
        op: MirUnaryOp,
        expr: Box<MirConstExpr>,
    },
    Binary {
        op: MirBinaryOp,
        left: Box<MirConstExpr>,
        right: Box<MirConstExpr>,
    },
    Unsupported,
}

impl From<&Expr> for MirConstExpr {
    fn from(expr: &Expr) -> Self {
        let span = expr.span();
        let kind = match expr {
            Expr::Ident(name, _) => MirConstExprKind::Ident(name.clone()),
            Expr::Int(value, _) => MirConstExprKind::Nat(*value),
            Expr::Bool(value, _) => MirConstExprKind::Bool(*value),
            Expr::Unary { op, expr, .. } => MirConstExprKind::Unary {
                op: MirUnaryOp::from(*op),
                expr: Box::new(Self::from(expr.as_ref())),
            },
            Expr::Binary {
                op, left, right, ..
            } => MirConstExprKind::Binary {
                op: MirBinaryOp::from(*op),
                left: Box::new(Self::from(left.as_ref())),
                right: Box::new(Self::from(right.as_ref())),
            },
            Expr::Group(inner, _) => return Self::from(inner.as_ref()),
            _ => MirConstExprKind::Unsupported,
        };
        Self { kind, span }
    }
}
