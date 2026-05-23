use std::collections::HashMap;
use syl_span::Span;
use syl_syntax::{BinaryOp, Expr, Pattern, SelectMode, TypeExpr, UnaryOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MirUnaryOp {
    Neg,
    Not,
    NotWord,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MirBinaryOp {
    Assign,
    OrOr,
    AndAnd,
    Eq,
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
    BitAnd,
    BitOr,
    BitXor,
    Unsupported,
}

impl From<BinaryOp> for MirBinaryOp {
    fn from(op: BinaryOp) -> Self {
        match op {
            BinaryOp::Assign => Self::Assign,
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
            BinaryOp::Field => Self::Field,
            BinaryOp::AndWord => Self::BitAnd,
            BinaryOp::OrWord => Self::BitOr,
            BinaryOp::XorWord => Self::BitXor,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MirSelectMode {
    Priority,
    Unique,
}

impl From<&SelectMode> for MirSelectMode {
    fn from(value: &SelectMode) -> Self {
        if value.is_unique() {
            Self::Unique
        } else {
            Self::Priority
        }
    }
}

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

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MirTypeRef {
    kind: MirTypeKind,
    span: Span,
}

#[allow(
    dead_code,
    reason = "Internal lowering helpers are retained here until the sema-side replacement lands."
)]
impl MirTypeRef {
    pub fn path_type(path: Vec<String>, span: Span) -> Self {
        Self {
            kind: MirTypeKind::Path(path),
            span,
        }
    }

    pub fn array_type(len: MirConstExpr, elem: MirTypeRef, span: Span) -> Self {
        Self {
            kind: MirTypeKind::Array {
                len,
                elem: Box::new(elem),
            },
            span,
        }
    }

    pub fn generic_type(base: MirTypeRef, args: Vec<MirTypeRef>, span: Span) -> Self {
        Self {
            kind: MirTypeKind::Generic {
                base: Box::new(base),
                args,
            },
            span,
        }
    }

    pub fn view_select_type(base: MirTypeRef, view: String, span: Span) -> Self {
        Self {
            kind: MirTypeKind::ViewSelect {
                base: Box::new(base),
                view,
            },
            span,
        }
    }

    pub fn unsupported(span: Span) -> Self {
        Self {
            kind: MirTypeKind::Unsupported,
            span,
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn path(&self) -> Option<&[String]> {
        match &self.kind {
            MirTypeKind::Path(path) => Some(path),
            _ => None,
        }
    }

    pub fn path_name(&self) -> Option<&str> {
        self.path()?.last().map(String::as_str)
    }

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

    pub fn args(&self) -> Option<&[MirTypeRef]> {
        match &self.kind {
            MirTypeKind::Generic { args, .. } => Some(args),
            MirTypeKind::ViewSelect { base, .. } => base.args(),
            _ => None,
        }
    }

    pub fn generic_base(&self) -> Option<&MirTypeRef> {
        match &self.kind {
            MirTypeKind::Generic { base, .. } => Some(base),
            _ => None,
        }
    }

    pub fn array(&self) -> Option<(&MirConstExpr, &MirTypeRef)> {
        match &self.kind {
            MirTypeKind::Array { len, elem } => Some((len, elem)),
            _ => None,
        }
    }

    pub fn view_select(&self) -> Option<(&MirTypeRef, &str)> {
        match &self.kind {
            MirTypeKind::ViewSelect { base, view } => Some((base, view)),
            _ => None,
        }
    }

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

    pub fn with_array_len(self, len: MirConstExpr, span: Span) -> Self {
        Self::array_type(len, self, span)
    }
}

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

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MirConstExpr {
    kind: MirConstExprKind,
    span: Span,
}

#[allow(
    dead_code,
    reason = "Internal lowering helpers are retained here until the sema-side replacement lands."
)]
impl MirConstExpr {
    pub fn ident_expr(name: String, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Ident(name),
            span,
        }
    }

    pub fn int(value: u64, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Int(value),
            span,
        }
    }

    pub fn bool_value_expr(value: bool, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Bool(value),
            span,
        }
    }

    pub fn unary_expr(op: MirUnaryOp, expr: MirConstExpr, span: Span) -> Self {
        Self {
            kind: MirConstExprKind::Unary {
                op,
                expr: Box::new(expr),
            },
            span,
        }
    }

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

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn ident(&self) -> Option<&str> {
        match &self.kind {
            MirConstExprKind::Ident(name) => Some(name),
            _ => None,
        }
    }

    pub fn int_value(&self) -> Option<u64> {
        match &self.kind {
            MirConstExprKind::Int(value) => Some(*value),
            _ => None,
        }
    }

    pub fn bool_value(&self) -> Option<bool> {
        match &self.kind {
            MirConstExprKind::Bool(value) => Some(*value),
            _ => None,
        }
    }

    pub fn unary(&self) -> Option<(MirUnaryOp, &MirConstExpr)> {
        match &self.kind {
            MirConstExprKind::Unary { op, expr } => Some((*op, expr)),
            _ => None,
        }
    }

    pub fn binary(&self) -> Option<(MirBinaryOp, &MirConstExpr, &MirConstExpr)> {
        match &self.kind {
            MirConstExprKind::Binary { op, left, right } => Some((*op, left, right)),
            _ => None,
        }
    }

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
            MirConstExprKind::Int(_)
            | MirConstExprKind::Bool(_)
            | MirConstExprKind::Unsupported => self.clone(),
        }
    }

    fn from_type_arg(ty: &MirTypeRef) -> Option<Self> {
        let name = ty.path_name()?;
        let kind = if let Ok(value) = name.parse::<u64>() {
            MirConstExprKind::Int(value)
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

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum MirConstExprKind {
    Ident(String),
    Int(u64),
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
            Expr::Int(value, _) => MirConstExprKind::Int(*value),
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
