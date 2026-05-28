use super::{Expr, Pattern, Stmt, TypeExpr};
use syl_span::Span;

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
            | Self::Assign { span, .. }
            | Self::Drive { span, .. }
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
