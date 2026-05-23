use crate::{
    mir::{MirBinaryOp, MirPattern, MirSelectMode, MirTypeRef, MirUnaryOp},
    source::{
        HirBlock, HirBodyExpr, HirExprNode, HirInstArg, HirMatchArm, HirNamedExpr, HirRegReset,
        HirSelectArm, HirStmt,
    },
};
use syl_hir::ExprId;
use syl_span::Span;

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabBlock {
    pub(crate) stmts: Vec<ElabStmt>,
    pub(crate) tail: Option<Box<ElabExpr>>,
}

impl From<&HirBlock> for ElabBlock {
    fn from(value: &HirBlock) -> Self {
        Self {
            stmts: value.stmts.iter().map(ElabStmt::from).collect(),
            tail: value
                .tail
                .as_ref()
                .map(|expr| Box::new(ElabExpr::from(expr.as_ref()))),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) enum ElabStmt {
    Error {
        span: Span,
    },
    Const {
        name: String,
        ty: Option<MirTypeRef>,
        value: ElabExpr,
        span: Span,
    },
    Let {
        span: Span,
    },
    Var {
        span: Span,
    },
    Alias {
        name: String,
        value: ElabExpr,
    },
    Signal {
        name: String,
        ty: Option<MirTypeRef>,
        value: Option<ElabExpr>,
        span: Span,
    },
    Reg {
        name: String,
        ty: Option<MirTypeRef>,
        reset: Option<ElabRegReset>,
        span: Span,
    },
    Next {
        name: String,
        value: ElabExpr,
        span: Span,
    },
    Inst {
        name: ElabExpr,
        callee: ElabExpr,
        span: Span,
    },
    While {
        span: Span,
    },
    ElabIf {
        cond: ElabExpr,
        then_block: ElabBlock,
        else_block: Option<ElabBlock>,
        span: Span,
    },
    ElabFor {
        name: String,
        range: ElabExpr,
        body: ElabBlock,
        span: Span,
    },
    Expr(ElabExpr),
    Return(Span),
}

impl From<&HirStmt> for ElabStmt {
    fn from(value: &HirStmt) -> Self {
        match value {
            HirStmt::Error { span } => Self::Error { span: *span },
            HirStmt::Const {
                name,
                ty,
                value,
                span,
                ..
            } => Self::Const {
                name: name.clone(),
                ty: ty.clone(),
                value: ElabExpr::from(value),
                span: *span,
            },
            HirStmt::Let { span, .. } => Self::Let { span: *span },
            HirStmt::Var { span, .. } => Self::Var { span: *span },
            HirStmt::Alias { name, value, .. } => Self::Alias {
                name: name.clone(),
                value: ElabExpr::from(value),
            },
            HirStmt::Signal {
                name,
                ty,
                value,
                span,
                ..
            } => Self::Signal {
                name: name.clone(),
                ty: ty.clone(),
                value: value.as_ref().map(ElabExpr::from),
                span: *span,
            },
            HirStmt::Reg {
                name,
                ty,
                reset,
                span,
                ..
            } => Self::Reg {
                name: name.clone(),
                ty: ty.clone(),
                reset: reset.as_ref().map(ElabRegReset::from),
                span: *span,
            },
            HirStmt::Next { name, value, span } => Self::Next {
                name: name.clone(),
                value: ElabExpr::from(value),
                span: *span,
            },
            HirStmt::Inst {
                name, callee, span, ..
            } => Self::Inst {
                name: ElabExpr::from(name),
                callee: ElabExpr::from(callee),
                span: *span,
            },
            HirStmt::While { span, .. } => Self::While { span: *span },
            HirStmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
            } => Self::ElabIf {
                cond: ElabExpr::from(cond),
                then_block: ElabBlock::from(then_block),
                else_block: else_block.as_ref().map(ElabBlock::from),
                span: *span,
            },
            HirStmt::ElabFor {
                name,
                range,
                body,
                span,
                ..
            } => Self::ElabFor {
                name: name.clone(),
                range: ElabExpr::from(range),
                body: ElabBlock::from(body),
                span: *span,
            },
            HirStmt::Expr(expr) => Self::Expr(ElabExpr::from(expr)),
            HirStmt::Return(_, span) => Self::Return(*span),
            _ => Self::Error { span: value.span() },
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabRegReset {
    pub(crate) domain: Option<ElabExpr>,
    pub(crate) value: ElabExpr,
    pub(crate) span: Span,
}

impl From<&HirRegReset> for ElabRegReset {
    fn from(value: &HirRegReset) -> Self {
        Self {
            domain: value.domain.as_ref().map(ElabExpr::from),
            value: ElabExpr::from(&value.value),
            span: value.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabExpr {
    pub(crate) id: ExprId,
    pub(crate) node: ElabExprNode,
    pub(crate) span: Span,
}

impl ElabExpr {
    pub(crate) fn id(&self) -> ExprId {
        self.id
    }

    pub(crate) fn span(&self) -> Span {
        self.span
    }
}

impl From<&HirBodyExpr> for ElabExpr {
    fn from(value: &HirBodyExpr) -> Self {
        let node = match &value.node {
            HirExprNode::Ident(name) => ElabExprNode::Ident(name.clone()),
            HirExprNode::Int(value) => ElabExprNode::Int(*value),
            HirExprNode::Str(value) => ElabExprNode::Str(value.clone()),
            HirExprNode::Bool(value) => ElabExprNode::Bool(*value),
            HirExprNode::Unary { op, expr } => ElabExprNode::Unary {
                op: MirUnaryOp::from(*op),
                expr: Box::new(ElabExpr::from(expr.as_ref())),
            },
            HirExprNode::Binary { op, left, right } => ElabExprNode::Binary {
                op: MirBinaryOp::from(*op),
                left: Box::new(ElabExpr::from(left.as_ref())),
                right: Box::new(ElabExpr::from(right.as_ref())),
            },
            HirExprNode::Call { callee, args } => ElabExprNode::Call {
                callee: Box::new(ElabExpr::from(callee.as_ref())),
                args: args.iter().map(ElabInstArg::from).collect(),
            },
            HirExprNode::GenericApp { callee, args } => ElabExprNode::GenericApp {
                callee: Box::new(ElabExpr::from(callee.as_ref())),
                args: args.clone(),
            },
            HirExprNode::Aggregate { ty, fields } => ElabExprNode::Aggregate {
                ty: ty.as_ref().clone(),
                fields: fields.iter().map(ElabNamedExpr::from).collect(),
            },
            HirExprNode::Field { base, field } => ElabExprNode::Field {
                base: Box::new(ElabExpr::from(base.as_ref())),
                field: field.clone(),
            },
            HirExprNode::Index { base, index } => ElabExprNode::Index {
                base: Box::new(ElabExpr::from(base.as_ref())),
                index: Box::new(ElabExpr::from(index.as_ref())),
            },
            HirExprNode::Group(expr) => {
                ElabExprNode::Group(Box::new(ElabExpr::from(expr.as_ref())))
            }
            HirExprNode::Block(block) => ElabExprNode::Block(ElabBlock::from(block)),
            HirExprNode::Match { expr, arms } => ElabExprNode::Match {
                expr: Box::new(ElabExpr::from(expr.as_ref())),
                arms: arms.iter().map(ElabMatchArm::from).collect(),
            },
            HirExprNode::Select { mode, arms } => ElabExprNode::Select {
                mode: *mode,
                arms: arms.iter().map(ElabSelectArm::from).collect(),
            },
            HirExprNode::Inst { callee, args } => ElabExprNode::Inst {
                callee: Box::new(ElabExpr::from(callee.as_ref())),
                args: args.iter().map(ElabInstArg::from).collect(),
            },
            HirExprNode::CompileError { message } => ElabExprNode::CompileError {
                message: Box::new(ElabExpr::from(message.as_ref())),
            },
            HirExprNode::Range { start, end } => ElabExprNode::Range {
                start: Box::new(ElabExpr::from(start.as_ref())),
                end: Box::new(ElabExpr::from(end.as_ref())),
            },
            HirExprNode::Unsupported => ElabExprNode::Unsupported,
            _ => ElabExprNode::Unsupported,
        };
        Self {
            id: value.id(),
            node,
            span: value.span(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) enum ElabExprNode {
    Ident(String),
    Int(u64),
    Str(String),
    Bool(bool),
    Unary {
        op: MirUnaryOp,
        expr: Box<ElabExpr>,
    },
    Binary {
        op: MirBinaryOp,
        left: Box<ElabExpr>,
        right: Box<ElabExpr>,
    },
    Call {
        callee: Box<ElabExpr>,
        args: Vec<ElabInstArg>,
    },
    GenericApp {
        callee: Box<ElabExpr>,
        args: Vec<MirTypeRef>,
    },
    Aggregate {
        ty: MirTypeRef,
        fields: Vec<ElabNamedExpr>,
    },
    Field {
        base: Box<ElabExpr>,
        field: String,
    },
    Index {
        base: Box<ElabExpr>,
        index: Box<ElabExpr>,
    },
    Group(Box<ElabExpr>),
    Block(ElabBlock),
    Match {
        expr: Box<ElabExpr>,
        arms: Vec<ElabMatchArm>,
    },
    Select {
        mode: MirSelectMode,
        arms: Vec<ElabSelectArm>,
    },
    Inst {
        callee: Box<ElabExpr>,
        args: Vec<ElabInstArg>,
    },
    CompileError {
        message: Box<ElabExpr>,
    },
    Range {
        start: Box<ElabExpr>,
        end: Box<ElabExpr>,
    },
    Unsupported,
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabNamedExpr {
    pub(crate) name: String,
    pub(crate) value: ElabExpr,
}

impl From<&HirNamedExpr> for ElabNamedExpr {
    fn from(value: &HirNamedExpr) -> Self {
        Self {
            name: value.name.clone(),
            value: ElabExpr::from(&value.value),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabInstArg {
    pub(crate) name: Option<String>,
    pub(crate) value: ElabExpr,
    pub(crate) span: Span,
}

impl From<&HirInstArg> for ElabInstArg {
    fn from(value: &HirInstArg) -> Self {
        Self {
            name: value.name.clone(),
            value: ElabExpr::from(&value.value),
            span: value.span(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabMatchArm {
    pub(crate) pattern: MirPattern,
    pub(crate) value: ElabExpr,
}

impl From<&HirMatchArm> for ElabMatchArm {
    fn from(value: &HirMatchArm) -> Self {
        Self {
            pattern: value.pattern.clone(),
            value: ElabExpr::from(&value.value),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabSelectArm {
    pub(crate) pattern: ElabExpr,
    pub(crate) value: ElabExpr,
}

impl From<&HirSelectArm> for ElabSelectArm {
    fn from(value: &HirSelectArm) -> Self {
        Self {
            pattern: ElabExpr::from(&value.pattern),
            value: ElabExpr::from(&value.value),
        }
    }
}
