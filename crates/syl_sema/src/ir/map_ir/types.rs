use crate::ir::mir::{
    MirBinaryOp, MirConstExpr, MirConstExprFacts, MirPattern, MirSelectMode, MirTypeRef, MirUnaryOp,
};
use crate::tir::{TirGenericArg, TirType};
use std::collections::HashMap;
use strum_macros::IntoStaticStr;
use syl_span::Span;
use syl_syntax::{BinaryOp, UnaryOp};

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum MapUnaryOp {
    #[strum(serialize = "-")]
    Neg,
    #[strum(serialize = "!")]
    Not,
    #[strum(serialize = "not")]
    NotWord,
    #[strum(serialize = "?")]
    Unsupported,
}

impl From<UnaryOp> for MapUnaryOp {
    fn from(op: UnaryOp) -> Self {
        match op {
            UnaryOp::Neg => Self::Neg,
            UnaryOp::Not => Self::Not,
            UnaryOp::NotWord => Self::NotWord,
            _ => Self::Unsupported,
        }
    }
}

impl From<MirUnaryOp> for MapUnaryOp {
    fn from(op: MirUnaryOp) -> Self {
        match op {
            MirUnaryOp::Neg => Self::Neg,
            MirUnaryOp::Not => Self::Not,
            MirUnaryOp::NotWord => Self::NotWord,
            MirUnaryOp::Unsupported => Self::Unsupported,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum MapBinaryOp {
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

impl From<BinaryOp> for MapBinaryOp {
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

impl From<MirBinaryOp> for MapBinaryOp {
    fn from(op: MirBinaryOp) -> Self {
        match op {
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
            MirBinaryOp::BitAnd => Self::BitAnd,
            MirBinaryOp::BitOr => Self::BitOr,
            MirBinaryOp::BitXor => Self::BitXor,
            MirBinaryOp::Unsupported => Self::Unsupported,
            _ => Self::Unsupported,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum MapSelectMode {
    #[strum(serialize = "priority")]
    Priority,
    #[strum(serialize = "unique")]
    Unique,
}

impl From<MirSelectMode> for MapSelectMode {
    fn from(mode: MirSelectMode) -> Self {
        match mode {
            MirSelectMode::Priority => Self::Priority,
            MirSelectMode::Unique => Self::Unique,
            _ => Self::Priority,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct MapTypeRef {
    kind: MapTypeKind,
    span: Span,
}

impl MapTypeRef {
    pub(crate) fn from_tir_type(ty: &TirType) -> Self {
        match ty {
            TirType::Named { name, args, .. } => {
                let base = Self {
                    kind: MapTypeKind::Path(vec![name.clone()]),
                    span: Span::default(),
                };
                if args.is_empty() {
                    base
                } else {
                    Self {
                        kind: MapTypeKind::Generic {
                            base: Box::new(base),
                            args: args.iter().map(Self::from_tir_generic_arg).collect(),
                        },
                        span: Span::default(),
                    }
                }
            }
            TirType::View { base, view } => Self {
                kind: MapTypeKind::ViewSelect {
                    base: Box::new(Self::from_tir_type(base)),
                    view: view.clone(),
                },
                span: Span::default(),
            },
            _ => Self {
                kind: MapTypeKind::Path(vec![ty.label()]),
                span: Span::default(),
            },
        }
    }

    pub(crate) fn from_const_label(label: String) -> Self {
        Self {
            kind: MapTypeKind::Path(vec![label]),
            span: Span::default(),
        }
    }

    fn from_tir_generic_arg(arg: &TirGenericArg) -> Self {
        match arg {
            TirGenericArg::Type(ty) => Self::from_tir_type(ty),
            TirGenericArg::Const(term) => Self::from_const_label(term.label()),
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }

    pub fn path(&self) -> Option<&[String]> {
        match &self.kind {
            MapTypeKind::Path(path) => Some(path),
            _ => None,
        }
    }

    pub fn path_name(&self) -> Option<&str> {
        self.path()?.last().map(String::as_str)
    }

    pub fn args(&self) -> Option<&[MapTypeRef]> {
        match &self.kind {
            MapTypeKind::Generic { args, .. } => Some(args),
            MapTypeKind::ViewSelect { base, .. } => base.args(),
            _ => None,
        }
    }

    pub fn generic_base(&self) -> Option<&MapTypeRef> {
        match &self.kind {
            MapTypeKind::Generic { base, .. } => Some(base),
            _ => None,
        }
    }

    pub fn array(&self) -> Option<(&MapConstExpr, &MapTypeRef)> {
        match &self.kind {
            MapTypeKind::Array { len, elem } => Some((len, elem)),
            _ => None,
        }
    }

    pub fn view_select(&self) -> Option<(&MapTypeRef, &str)> {
        match &self.kind {
            MapTypeKind::ViewSelect { base, view } => Some((base, view)),
            _ => None,
        }
    }

    pub fn subst(&self, replacements: &HashMap<String, MapTypeRef>) -> Self {
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
pub struct MapConstExpr {
    kind: MapConstExprKind,
    span: Span,
}

impl MapConstExpr {
    pub fn span(&self) -> Span {
        self.span
    }

    pub fn ident(&self) -> Option<&str> {
        match &self.kind {
            MapConstExprKind::Ident(name) => Some(name),
            _ => None,
        }
    }

    pub fn nat_value(&self) -> Option<u64> {
        match &self.kind {
            MapConstExprKind::Nat(_, value) => Some(*value),
            _ => None,
        }
    }

    pub fn bool_value(&self) -> Option<bool> {
        match &self.kind {
            MapConstExprKind::Bool(_, value) => Some(*value),
            _ => None,
        }
    }

    pub fn unary(&self) -> Option<(MapUnaryOp, &MapConstExpr)> {
        match &self.kind {
            MapConstExprKind::Unary { op, expr } => Some((*op, expr)),
            _ => None,
        }
    }

    pub fn binary(&self) -> Option<(MapBinaryOp, &MapConstExpr, &MapConstExpr)> {
        match &self.kind {
            MapConstExprKind::Binary { op, left, right } => Some((*op, left, right)),
            _ => None,
        }
    }

    pub fn fact_key(&self) -> String {
        match &self.kind {
            MapConstExprKind::Ident(name) | MapConstExprKind::Opaque(name) => name.clone(),
            MapConstExprKind::Nat(text, _) | MapConstExprKind::Bool(text, _) => text.clone(),
            MapConstExprKind::Unary { op, expr } => {
                format!("({}{})", <&'static str>::from(*op), expr.fact_key())
            }
            MapConstExprKind::Binary { op, left, right } => format!(
                "({} {} {})",
                left.fact_key(),
                <&'static str>::from(*op),
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
pub struct MapGenericArg {
    ty: MapTypeRef,
}

impl MapGenericArg {
    pub fn ty(&self) -> &MapTypeRef {
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

impl From<&TirGenericArg> for MapGenericArg {
    fn from(arg: &TirGenericArg) -> Self {
        let ty = match arg {
            TirGenericArg::Type(ty) => MapTypeRef::from_tir_type(ty),
            TirGenericArg::Const(term) => MapTypeRef::from_const_label(term.label()),
        };
        Self { ty }
    }
}

#[derive(Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum MapPattern {
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
            _ => Self::Unsupported,
        }
    }
}
