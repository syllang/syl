use super::{MirPattern, MirSelectMode, MirTypeRef};
use crate::{ExprId, LocalId};
use syl_span::Span;
use syl_syntax::{
    BinaryOp, Block, CallArg, Expr, MatchArm, NamedExpr, RegReset, SelectArm, Stmt, UnaryOp,
};

#[derive(Clone)]
#[non_exhaustive]
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub tail: Option<Box<HirExpr>>,
    pub span: Span,
}

impl HirBlock {
    pub(crate) fn from_syntax(block: &Block) -> Self {
        Self {
            stmts: block.stmts.iter().map(HirStmt::from_syntax).collect(),
            tail: block
                .tail
                .as_ref()
                .map(|expr| Box::new(HirExpr::from_syntax(expr))),
            span: block.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub enum HirStmt {
    Error {
        span: Span,
    },
    Const {
        id: Option<LocalId>,
        name: String,
        ty: Option<MirTypeRef>,
        value: HirExpr,
        span: Span,
    },
    Let {
        id: Option<LocalId>,
        name: String,
        ty: Option<MirTypeRef>,
        value: Option<HirExpr>,
        span: Span,
    },
    Var {
        id: Option<LocalId>,
        name: String,
        ty: Option<MirTypeRef>,
        value: Option<HirExpr>,
        span: Span,
    },
    Signal {
        id: Option<LocalId>,
        name: String,
        ty: Option<MirTypeRef>,
        value: Option<HirExpr>,
        span: Span,
    },
    Reg {
        id: Option<LocalId>,
        name: String,
        ty: Option<MirTypeRef>,
        reset: Option<HirRegReset>,
        span: Span,
    },
    Next {
        name: String,
        value: HirExpr,
        span: Span,
    },
    While {
        cond: HirExpr,
        body: HirBlock,
        span: Span,
    },
    ElabIf {
        cond: HirExpr,
        then_block: HirBlock,
        else_block: Option<HirBlock>,
        span: Span,
    },
    ElabFor {
        id: Option<LocalId>,
        name: String,
        range: HirExpr,
        body: HirBlock,
        span: Span,
    },
    Expr(HirExpr),
    Return(Option<HirExpr>, Span),
}

impl HirStmt {
    pub fn span(&self) -> Span {
        match self {
            HirStmt::Error { span }
            | HirStmt::Const { span, .. }
            | HirStmt::Let { span, .. }
            | HirStmt::Var { span, .. }
            | HirStmt::Signal { span, .. }
            | HirStmt::Reg { span, .. }
            | HirStmt::Next { span, .. }
            | HirStmt::While { span, .. }
            | HirStmt::ElabIf { span, .. }
            | HirStmt::ElabFor { span, .. }
            | HirStmt::Return(_, span) => *span,
            HirStmt::Expr(expr) => expr.span(),
        }
    }

    fn from_syntax(stmt: &Stmt) -> Self {
        match stmt {
            Stmt::Error { span } => HirStmt::Error { span: *span },
            Stmt::Const {
                name,
                ty,
                value,
                span,
            } => HirStmt::Const {
                id: None,
                name: name.clone(),
                ty: ty.as_ref().map(MirTypeRef::from),
                value: HirExpr::from_syntax(value),
                span: *span,
            },
            Stmt::Let {
                name,
                ty,
                value,
                span,
            } => HirStmt::Let {
                id: None,
                name: name.clone(),
                ty: ty.as_ref().map(MirTypeRef::from),
                value: value.as_ref().map(HirExpr::from_syntax),
                span: *span,
            },
            Stmt::Var {
                name,
                ty,
                value,
                span,
            } => HirStmt::Var {
                id: None,
                name: name.clone(),
                ty: ty.as_ref().map(MirTypeRef::from),
                value: value.as_ref().map(HirExpr::from_syntax),
                span: *span,
            },
            Stmt::Signal {
                name,
                ty,
                value,
                span,
            } => HirStmt::Signal {
                id: None,
                name: name.clone(),
                ty: ty.as_ref().map(MirTypeRef::from),
                value: value.as_ref().map(HirExpr::from_syntax),
                span: *span,
            },
            Stmt::Reg {
                name,
                ty,
                reset,
                span,
            } => HirStmt::Reg {
                id: None,
                name: name.clone(),
                ty: ty.as_ref().map(MirTypeRef::from),
                reset: reset.as_ref().map(HirRegReset::from_syntax),
                span: *span,
            },
            Stmt::Next { name, value, span } => HirStmt::Next {
                name: name.clone(),
                value: HirExpr::from_syntax(value),
                span: *span,
            },
            Stmt::While { cond, body, span } => HirStmt::While {
                cond: HirExpr::from_syntax(cond),
                body: HirBlock::from_syntax(body),
                span: *span,
            },
            Stmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
            } => HirStmt::ElabIf {
                cond: HirExpr::from_syntax(cond),
                then_block: HirBlock::from_syntax(then_block),
                else_block: else_block.as_ref().map(HirBlock::from_syntax),
                span: *span,
            },
            Stmt::ElabFor {
                name,
                range,
                body,
                span,
            } => HirStmt::ElabFor {
                id: None,
                name: name.clone(),
                range: HirExpr::from_syntax(range),
                body: HirBlock::from_syntax(body),
                span: *span,
            },
            Stmt::Expr(expr) => HirStmt::Expr(HirExpr::from_syntax(expr)),
            Stmt::Return(value, span) => {
                HirStmt::Return(value.as_ref().map(HirExpr::from_syntax), *span)
            }
            _ => HirStmt::Error { span: stmt.span() },
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirRegReset {
    pub domain: Option<HirExpr>,
    pub value: HirExpr,
    pub span: Span,
}

impl HirRegReset {
    fn from_syntax(reset: &RegReset) -> Self {
        Self {
            domain: reset.domain.as_ref().map(HirExpr::from_syntax),
            value: HirExpr::from_syntax(&reset.value),
            span: reset.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirExpr {
    pub id: ExprId,
    pub node: HirExprNode,
    pub span: Span,
}

impl HirExpr {
    fn unallocated_id() -> ExprId {
        ExprId::new(usize::MAX)
    }

    pub(crate) fn from_syntax(expr: &Expr) -> Self {
        let span = expr.span();
        let node = match expr {
            Expr::Ident(name, _) => HirExprNode::Ident(name.clone()),
            Expr::Int(value, _) => HirExprNode::Int(*value),
            Expr::Str(value, _) => HirExprNode::Str(value.clone()),
            Expr::Bool(value, _) => HirExprNode::Bool(*value),
            Expr::Unary { op, expr, .. } => HirExprNode::Unary {
                op: *op,
                expr: Box::new(HirExpr::from_syntax(expr)),
            },
            Expr::Binary {
                op, left, right, ..
            } => HirExprNode::Binary {
                op: *op,
                left: Box::new(HirExpr::from_syntax(left)),
                right: Box::new(HirExpr::from_syntax(right)),
            },
            Expr::Call { callee, args, .. } => HirExprNode::Call {
                callee: Box::new(HirExpr::from_syntax(callee)),
                args: args.iter().map(HirCallArg::from_syntax).collect(),
            },
            Expr::GenericApp { callee, args, .. } => HirExprNode::GenericApp {
                callee: Box::new(HirExpr::from_syntax(callee)),
                args: args.iter().map(MirTypeRef::from).collect(),
            },
            Expr::Aggregate { ty, fields, .. } => HirExprNode::Aggregate {
                ty: Box::new(MirTypeRef::from(ty.as_ref())),
                fields: fields.iter().map(HirNamedExpr::from_syntax).collect(),
            },
            Expr::Field { base, field, .. } => HirExprNode::Field {
                base: Box::new(HirExpr::from_syntax(base)),
                field: field.clone(),
            },
            Expr::Index { base, index, .. } => HirExprNode::Index {
                base: Box::new(HirExpr::from_syntax(base)),
                index: Box::new(HirExpr::from_syntax(index)),
            },
            Expr::Group(expr, _) => HirExprNode::Group(Box::new(HirExpr::from_syntax(expr))),
            Expr::Block(block) => HirExprNode::Block(HirBlock::from_syntax(block)),
            Expr::Match { expr, arms, .. } => HirExprNode::Match {
                expr: Box::new(HirExpr::from_syntax(expr)),
                arms: arms.iter().map(HirMatchArm::from_syntax).collect(),
            },
            Expr::Select { mode, arms, .. } => HirExprNode::Select {
                mode: MirSelectMode::from(*mode),
                arms: arms.iter().map(HirSelectArm::from_syntax).collect(),
            },
            Expr::Place { callee, args, .. } => HirExprNode::Place {
                callee: Box::new(HirExpr::from_syntax(callee)),
                args: args.iter().map(HirCallArg::from_syntax).collect(),
            },
            Expr::For {
                name, range, body, ..
            } => HirExprNode::For {
                id: None,
                name: name.clone(),
                range: Box::new(HirExpr::from_syntax(range)),
                body: HirBlock::from_syntax(body),
            },
            Expr::CompileError { message, .. } => HirExprNode::CompileError {
                message: Box::new(HirExpr::from_syntax(message)),
            },
            Expr::Range { start, end, .. } => HirExprNode::Range {
                start: Box::new(HirExpr::from_syntax(start)),
                end: Box::new(HirExpr::from_syntax(end)),
            },
            _ => HirExprNode::Unsupported,
        };
        HirExpr {
            id: Self::unallocated_id(),
            node,
            span,
        }
    }

    pub fn id(&self) -> ExprId {
        self.id
    }

    #[allow(
        dead_code,
        reason = "Internal lowering still allocates IDs after HIR construction."
    )]
    pub(crate) fn set_id(&mut self, id: ExprId) {
        debug_assert_eq!(
            self.id,
            Self::unallocated_id(),
            "HIR expression ID allocated more than once"
        );
        self.id = id;
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub enum HirExprNode {
    Ident(String),
    Int(u64),
    Str(String),
    Bool(bool),
    Unary {
        op: UnaryOp,
        expr: Box<HirExpr>,
    },
    Binary {
        op: BinaryOp,
        left: Box<HirExpr>,
        right: Box<HirExpr>,
    },
    Call {
        callee: Box<HirExpr>,
        args: Vec<HirCallArg>,
    },
    GenericApp {
        callee: Box<HirExpr>,
        args: Vec<MirTypeRef>,
    },
    Aggregate {
        ty: Box<MirTypeRef>,
        fields: Vec<HirNamedExpr>,
    },
    Field {
        base: Box<HirExpr>,
        field: String,
    },
    Index {
        base: Box<HirExpr>,
        index: Box<HirExpr>,
    },
    Group(Box<HirExpr>),
    Block(HirBlock),
    Match {
        expr: Box<HirExpr>,
        arms: Vec<HirMatchArm>,
    },
    Select {
        mode: MirSelectMode,
        arms: Vec<HirSelectArm>,
    },
    Place {
        callee: Box<HirExpr>,
        args: Vec<HirCallArg>,
    },
    For {
        id: Option<LocalId>,
        name: String,
        range: Box<HirExpr>,
        body: HirBlock,
    },
    CompileError {
        message: Box<HirExpr>,
    },
    Range {
        start: Box<HirExpr>,
        end: Box<HirExpr>,
    },
    Unsupported,
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirNamedExpr {
    pub name: String,
    pub value: HirExpr,
    #[allow(
        dead_code,
        reason = "HIR preserves field spans for diagnostics and LSP source maps."
    )]
    pub(crate) span: Span,
}

impl HirNamedExpr {
    fn from_syntax(expr: &NamedExpr) -> Self {
        Self {
            name: expr.name.clone(),
            value: HirExpr::from_syntax(&expr.value),
            span: expr.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirCallArg {
    pub name: Option<String>,
    pub value: HirExpr,
    #[allow(
        dead_code,
        reason = "HIR retains argument spans for diagnostics and source mapping."
    )]
    pub(crate) span: Span,
}

impl HirCallArg {
    fn from_syntax(arg: &CallArg) -> Self {
        Self {
            name: arg.name.clone(),
            value: HirExpr::from_syntax(&arg.value),
            span: arg.span,
        }
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirMatchArm {
    pub pattern: MirPattern,
    pub value: HirExpr,
    #[allow(
        dead_code,
        reason = "HIR preserves arm spans for diagnostics and LSP source maps."
    )]
    pub(crate) span: Span,
}

impl HirMatchArm {
    fn from_syntax(arm: &MatchArm) -> Self {
        Self {
            pattern: MirPattern::from(&arm.pattern),
            value: HirExpr::from_syntax(&arm.value),
            span: arm.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirSelectArm {
    pub pattern: HirExpr,
    pub value: HirExpr,
    #[allow(
        dead_code,
        reason = "HIR preserves arm spans for diagnostics and LSP source maps."
    )]
    pub(crate) span: Span,
}

impl HirSelectArm {
    fn from_syntax(arm: &SelectArm) -> Self {
        Self {
            pattern: HirExpr::from_syntax(&arm.pattern),
            value: HirExpr::from_syntax(&arm.value),
            span: arm.span,
        }
    }
}
