use crate::mir::{MirBinaryOp, MirConstExpr, MirPattern, MirSelectMode, MirTypeRef, MirUnaryOp};
use syl_span::Span;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum MapUnaryOp {
    Neg,
    Not,
    NotWord,
    Unsupported,
}

impl From<MirUnaryOp> for MapUnaryOp {
    fn from(op: MirUnaryOp) -> Self {
        match op {
            MirUnaryOp::Neg => Self::Neg,
            MirUnaryOp::Not => Self::Not,
            MirUnaryOp::NotWord => Self::NotWord,
            MirUnaryOp::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum MapBinaryOp {
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

impl From<MirBinaryOp> for MapBinaryOp {
    fn from(op: MirBinaryOp) -> Self {
        match op {
            MirBinaryOp::Assign => Self::Assign,
            MirBinaryOp::OrOr => Self::OrOr,
            MirBinaryOp::AndAnd => Self::AndAnd,
            MirBinaryOp::Eq => Self::Eq,
            MirBinaryOp::NotEq => Self::NotEq,
            MirBinaryOp::Lt => Self::Lt,
            MirBinaryOp::LtEq => Self::LtEq,
            MirBinaryOp::Gt => Self::Gt,
            MirBinaryOp::GtEq => Self::GtEq,
            MirBinaryOp::Add => Self::Add,
            MirBinaryOp::Sub => Self::Sub,
            MirBinaryOp::Mul => Self::Mul,
            MirBinaryOp::Div => Self::Div,
            MirBinaryOp::Rem => Self::Rem,
            MirBinaryOp::Shl => Self::Shl,
            MirBinaryOp::Field => Self::Field,
            MirBinaryOp::BitAnd => Self::BitAnd,
            MirBinaryOp::BitOr => Self::BitOr,
            MirBinaryOp::BitXor => Self::BitXor,
            MirBinaryOp::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum MapSelectMode {
    Priority,
    Unique,
}

impl From<MirSelectMode> for MapSelectMode {
    fn from(mode: MirSelectMode) -> Self {
        match mode {
            MirSelectMode::Priority => Self::Priority,
            MirSelectMode::Unique => Self::Unique,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub(crate) struct MapTypeRef {
    kind: MapTypeKind,
    span: Span,
}

impl MapTypeRef {
    pub(crate) fn span(&self) -> Span {
        self.span
    }

    pub(crate) fn path(&self) -> Option<&[String]> {
        match &self.kind {
            MapTypeKind::Path(path) => Some(path),
            _ => None,
        }
    }

    pub(crate) fn path_name(&self) -> Option<&str> {
        self.path()?.last().map(String::as_str)
    }

    pub(crate) fn args(&self) -> Option<&[MapTypeRef]> {
        match &self.kind {
            MapTypeKind::Generic { args, .. } => Some(args),
            MapTypeKind::ViewSelect { base, .. } => base.args(),
            _ => None,
        }
    }

    pub(crate) fn generic_base(&self) -> Option<&MapTypeRef> {
        match &self.kind {
            MapTypeKind::Generic { base, .. } => Some(base),
            _ => None,
        }
    }

    pub(crate) fn array(&self) -> Option<(&MapConstExpr, &MapTypeRef)> {
        match &self.kind {
            MapTypeKind::Array { len, elem } => Some((len, elem)),
            _ => None,
        }
    }

    pub(crate) fn view_select(&self) -> Option<(&MapTypeRef, &str)> {
        match &self.kind {
            MapTypeKind::ViewSelect { base, view } => Some((base, view)),
            _ => None,
        }
    }

    pub(crate) fn subst(&self, replacements: &HashMap<String, MapTypeRef>) -> Self {
        match &self.kind {
            MapTypeKind::Path(path) if path.len() == 1 => replacements
                .get(&path[0])
                .cloned()
                .unwrap_or_else(|| self.clone()),
            MapTypeKind::Path(_) | MapTypeKind::Unsupported => self.clone(),
            MapTypeKind::Array { len, elem } => Self {
                kind: MapTypeKind::Array {
                    len: len.subst_type_vars(replacements),
                    elem: Box::new(elem.subst(replacements)),
                },
                span: self.span,
            },
            MapTypeKind::Generic { base, args } => Self {
                kind: MapTypeKind::Generic {
                    base: Box::new(base.subst(replacements)),
                    args: args.iter().map(|arg| arg.subst(replacements)).collect(),
                },
                span: self.span,
            },
            MapTypeKind::ViewSelect { base, view } => Self {
                kind: MapTypeKind::ViewSelect {
                    base: Box::new(base.subst(replacements)),
                    view: view.clone(),
                },
                span: self.span,
            },
        }
    }
}

impl From<&MirTypeRef> for MapTypeRef {
    fn from(ty: &MirTypeRef) -> Self {
        if let Some(path) = ty.path() {
            return Self {
                kind: MapTypeKind::Path(path.to_vec()),
                span: ty.span(),
            };
        }
        if let Some((len, elem)) = ty.array() {
            return Self {
                kind: MapTypeKind::Array {
                    len: MapConstExpr::from(len),
                    elem: Box::new(Self::from(elem)),
                },
                span: ty.span(),
            };
        }
        if let Some(base) = ty.generic_base() {
            return Self {
                kind: MapTypeKind::Generic {
                    base: Box::new(Self::from(base)),
                    args: ty
                        .args()
                        .unwrap_or_default()
                        .iter()
                        .map(Self::from)
                        .collect(),
                },
                span: ty.span(),
            };
        }
        if let Some((base, view)) = ty.view_select() {
            return Self {
                kind: MapTypeKind::ViewSelect {
                    base: Box::new(Self::from(base)),
                    view: view.to_string(),
                },
                span: ty.span(),
            };
        }
        Self {
            kind: MapTypeKind::Unsupported,
            span: ty.span(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
enum MapTypeKind {
    Path(Vec<String>),
    Array {
        len: MapConstExpr,
        elem: Box<MapTypeRef>,
    },
    Generic {
        base: Box<MapTypeRef>,
        args: Vec<MapTypeRef>,
    },
    ViewSelect {
        base: Box<MapTypeRef>,
        view: String,
    },
    Unsupported,
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub(crate) struct MapConstExpr {
    kind: MapConstExprKind,
    span: Span,
}

impl MapConstExpr {
    pub(crate) fn span(&self) -> Span {
        self.span
    }

    pub(crate) fn ident(&self) -> Option<&str> {
        match &self.kind {
            MapConstExprKind::Ident(name) => Some(name),
            _ => None,
        }
    }

    pub(crate) fn nat_value(&self) -> Option<u64> {
        match &self.kind {
            MapConstExprKind::Nat(_, value) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn bool_value(&self) -> Option<bool> {
        match &self.kind {
            MapConstExprKind::Bool(_, value) => Some(*value),
            _ => None,
        }
    }

    pub(crate) fn unary(&self) -> Option<(MapUnaryOp, &MapConstExpr)> {
        match &self.kind {
            MapConstExprKind::Unary { op, expr } => Some((*op, expr)),
            _ => None,
        }
    }

    pub(crate) fn binary(&self) -> Option<(MapBinaryOp, &MapConstExpr, &MapConstExpr)> {
        match &self.kind {
            MapConstExprKind::Binary { op, left, right } => Some((*op, left, right)),
            _ => None,
        }
    }

    pub(crate) fn fact_key(&self) -> String {
        match &self.kind {
            MapConstExprKind::Ident(name) | MapConstExprKind::Opaque(name) => name.clone(),
            MapConstExprKind::Nat(text, _) | MapConstExprKind::Bool(text, _) => text.clone(),
            MapConstExprKind::Unary { op, expr } => {
                format!("({}{})", Self::unary_symbol(*op), expr.fact_key())
            }
            MapConstExprKind::Binary { op, left, right } => format!(
                "({} {} {})",
                left.fact_key(),
                Self::binary_symbol(*op),
                right.fact_key()
            ),
        }
    }

    fn subst_type_vars(&self, replacements: &HashMap<String, MapTypeRef>) -> Self {
        match &self.kind {
            MapConstExprKind::Ident(name) => replacements
                .get(name)
                .and_then(Self::from_type_arg)
                .unwrap_or_else(|| self.clone()),
            MapConstExprKind::Unary { op, expr } => Self {
                kind: MapConstExprKind::Unary {
                    op: *op,
                    expr: Box::new(expr.subst_type_vars(replacements)),
                },
                span: self.span,
            },
            MapConstExprKind::Binary { op, left, right } => Self {
                kind: MapConstExprKind::Binary {
                    op: *op,
                    left: Box::new(left.subst_type_vars(replacements)),
                    right: Box::new(right.subst_type_vars(replacements)),
                },
                span: self.span,
            },
            MapConstExprKind::Nat(_, _)
            | MapConstExprKind::Bool(_, _)
            | MapConstExprKind::Opaque(_) => self.clone(),
        }
    }

    fn from_type_arg(ty: &MapTypeRef) -> Option<Self> {
        let name = ty.path_name()?;
        let kind = if let Ok(value) = name.parse::<u64>() {
            MapConstExprKind::Nat(name.to_string(), value)
        } else if name == "true" {
            MapConstExprKind::Bool(name.to_string(), true)
        } else if name == "false" {
            MapConstExprKind::Bool(name.to_string(), false)
        } else {
            MapConstExprKind::Ident(name.to_string())
        };
        Some(Self {
            kind,
            span: ty.span(),
        })
    }

    fn unary_symbol(op: MapUnaryOp) -> &'static str {
        match op {
            MapUnaryOp::Neg => "-",
            MapUnaryOp::Not => "!",
            MapUnaryOp::NotWord => "not",
            MapUnaryOp::Unsupported => "?",
        }
    }

    fn binary_symbol(op: MapBinaryOp) -> &'static str {
        match op {
            MapBinaryOp::Assign => "=",
            MapBinaryOp::OrOr => "||",
            MapBinaryOp::AndAnd => "&&",
            MapBinaryOp::Eq => "==",
            MapBinaryOp::NotEq => "!=",
            MapBinaryOp::Lt => "<",
            MapBinaryOp::LtEq => "<=",
            MapBinaryOp::Gt => ">",
            MapBinaryOp::GtEq => ">=",
            MapBinaryOp::Add => "+",
            MapBinaryOp::Sub => "-",
            MapBinaryOp::Mul => "*",
            MapBinaryOp::Div => "/",
            MapBinaryOp::Rem => "%",
            MapBinaryOp::Shl => "<<",
            MapBinaryOp::Field => ".",
            MapBinaryOp::BitAnd => "and",
            MapBinaryOp::BitOr => "or",
            MapBinaryOp::BitXor => "xor",
            MapBinaryOp::Unsupported => "?",
        }
    }
}

impl From<&MirConstExpr> for MapConstExpr {
    fn from(expr: &MirConstExpr) -> Self {
        if let Some(value) = expr.nat_value() {
            return Self {
                kind: MapConstExprKind::Nat(value.to_string(), value),
                span: expr.span(),
            };
        }
        if let Some(value) = expr.bool_value() {
            return Self {
                kind: MapConstExprKind::Bool(value.to_string(), value),
                span: expr.span(),
            };
        }
        if let Some(name) = expr.ident() {
            return Self {
                kind: MapConstExprKind::Ident(name.to_string()),
                span: expr.span(),
            };
        }
        if let Some((op, expr)) = expr.unary() {
            return Self {
                kind: MapConstExprKind::Unary {
                    op: MapUnaryOp::from(op),
                    expr: Box::new(Self::from(expr)),
                },
                span: expr.span(),
            };
        }
        if let Some((op, left, right)) = expr.binary() {
            return Self {
                kind: MapConstExprKind::Binary {
                    op: MapBinaryOp::from(op),
                    left: Box::new(Self::from(left)),
                    right: Box::new(Self::from(right)),
                },
                span: expr.span(),
            };
        }
        Self {
            kind: MapConstExprKind::Opaque(expr.fact_key()),
            span: expr.span(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
enum MapConstExprKind {
    Ident(String),
    Nat(String, u64),
    Bool(String, bool),
    Unary {
        op: MapUnaryOp,
        expr: Box<MapConstExpr>,
    },
    Binary {
        op: MapBinaryOp,
        left: Box<MapConstExpr>,
        right: Box<MapConstExpr>,
    },
    Opaque(String),
}

#[derive(Debug, PartialEq)]
#[non_exhaustive]
pub(crate) struct MapGenericArg {
    ty: MapTypeRef,
}

impl MapGenericArg {
    pub(crate) fn ty(&self) -> &MapTypeRef {
        &self.ty
    }
}

impl From<&MirTypeRef> for MapGenericArg {
    fn from(ty: &MirTypeRef) -> Self {
        Self {
            ty: MapTypeRef::from(ty),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub(crate) enum MapPattern {
    Wildcard,
    Ident(String),
    Int(u64),
    Bool(bool),
    Path(Vec<String>),
    Unsupported,
}

impl From<&MirPattern> for MapPattern {
    fn from(pattern: &MirPattern) -> Self {
        match pattern {
            MirPattern::Wildcard(_) => Self::Wildcard,
            MirPattern::Ident(name, _) => Self::Ident(name.clone()),
            MirPattern::Int(value, _) => Self::Int(*value),
            MirPattern::Bool(value, _) => Self::Bool(*value),
            MirPattern::Path(path, _) => Self::Path(path.clone()),
            MirPattern::Unsupported(_) => Self::Unsupported,
        }
    }
}
