use super::{MirPattern, MirSelectMode, MirTypeRef};
use crate::{ExprId, LocalId};
use syl_span::Span;
use syl_syntax::{
    BinaryOp, Block, CallArg, Expr, MatchArm, NamedExpr, RegReset, SelectArm, Stmt, UnaryOp,
};

/// Tracks whether a HIR block was lowered from a `fn` body (Software) or a
/// `cell` body (Hardware). This affects which statements are valid:
///
/// - **Software** context: allows `let`, `var`, `while`, `return`, `ElabIf`,
///   `ElabFor` — general-purpose software-like constructs for elaboration.
/// - **Hardware** context: allows `signal`, `reg`, `drive`, `assign`, `next` —
///   hardware-specific statements that generate physical circuits.
///
/// When lowering, invalid statement combinations produce `Stmt::Error` nodes.
/// The distinction is dropped after HIR construction; EIR does not track it.
#[derive(Clone, Copy)]
enum HirBlockContext {
    Hardware,
    Software,
}

/// A block of HIR statements with an optional tail expression.
#[derive(Clone)]
#[non_exhaustive]
pub struct HirBlock {
    pub stmts: Vec<HirStmt>,
    pub tail: Option<Box<HirExpr>>,
    pub span: Span,
}

impl HirBlock {
    pub(crate) fn from_hardware_syntax(block: &Block) -> Self {
        Self::from_syntax_with_context(block, HirBlockContext::Hardware)
    }

    pub(crate) fn from_software_syntax(block: &Block) -> Self {
        Self::from_syntax_with_context(block, HirBlockContext::Software)
    }

    fn from_syntax_with_context(block: &Block, context: HirBlockContext) -> Self {
        Self {
            stmts: block
                .stmts
                .iter()
                .map(|stmt| HirStmt::from_syntax(stmt, context))
                .collect(),
            tail: block
                .tail
                .as_ref()
                .map(|expr| Box::new(HirExpr::from_syntax_with_context(expr, context))),
            span: block.span,
        }
    }
}

/// A statement in the HIR, mirroring the AST `Stmt` with resolved IDs and types.
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
    Assign {
        target: HirExpr,
        value: HirExpr,
        span: Span,
    },
    Drive {
        target: HirExpr,
        value: HirExpr,
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
    /// Returns the source span of this statement.
    pub fn span(&self) -> Span {
        match self {
            HirStmt::Error { span }
            | HirStmt::Const { span, .. }
            | HirStmt::Let { span, .. }
            | HirStmt::Var { span, .. }
            | HirStmt::Signal { span, .. }
            | HirStmt::Reg { span, .. }
            | HirStmt::Assign { span, .. }
            | HirStmt::Drive { span, .. }
            | HirStmt::Next { span, .. }
            | HirStmt::While { span, .. }
            | HirStmt::ElabIf { span, .. }
            | HirStmt::ElabFor { span, .. }
            | HirStmt::Return(_, span) => *span,
            HirStmt::Expr(expr) => expr.span(),
        }
    }

    fn from_syntax(stmt: &Stmt, context: HirBlockContext) -> Self {
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
                value: HirExpr::from_syntax_with_context(value, context),
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
                value: value
                    .as_ref()
                    .map(|expr| HirExpr::from_syntax_with_context(expr, context)),
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
                value: value
                    .as_ref()
                    .map(|expr| HirExpr::from_syntax_with_context(expr, context)),
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
                value: value
                    .as_ref()
                    .map(|expr| HirExpr::from_syntax_with_context(expr, context)),
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
            Stmt::Assign {
                target,
                value,
                span,
            } => HirStmt::Assign {
                target: HirExpr::from_syntax_with_context(target, context),
                value: HirExpr::from_syntax_with_context(value, context),
                span: *span,
            },
            Stmt::Drive {
                target,
                value,
                span,
            } => HirStmt::Drive {
                target: HirExpr::from_syntax_with_context(target, context),
                value: HirExpr::from_syntax_with_context(value, context),
                span: *span,
            },
            Stmt::Next { name, value, span } => HirStmt::Next {
                name: name.clone(),
                value: HirExpr::from_syntax_with_context(value, context),
                span: *span,
            },
            Stmt::While { cond, body, span } => HirStmt::While {
                cond: HirExpr::from_syntax_with_context(cond, context),
                body: HirBlock::from_syntax_with_context(body, context),
                span: *span,
            },
            Stmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
            } => HirStmt::ElabIf {
                cond: HirExpr::from_syntax_with_context(cond, context),
                then_block: HirBlock::from_syntax_with_context(then_block, context),
                else_block: else_block
                    .as_ref()
                    .map(|block| HirBlock::from_syntax_with_context(block, context)),
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
                range: HirExpr::from_syntax_with_context(range, context),
                body: HirBlock::from_syntax_with_context(body, context),
                span: *span,
            },
            Stmt::Expr(expr) => HirStmt::Expr(HirExpr::from_syntax_with_context(expr, context)),
            Stmt::Return(value, span) => HirStmt::Return(
                value
                    .as_ref()
                    .map(|expr| HirExpr::from_syntax_with_context(expr, context)),
                *span,
            ),
            _ => HirStmt::Error { span: stmt.span() },
        }
    }
}

/// Reset specification for a register in the HIR.
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
            domain: reset
                .domain
                .as_ref()
                .map(|expr| HirExpr::from_syntax_with_context(expr, HirBlockContext::Hardware)),
            value: HirExpr::from_syntax_with_context(&reset.value, HirBlockContext::Hardware),
            span: reset.span,
        }
    }
}

/// A single expression in the HIR with its node data and source span.
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
        Self::from_syntax_with_context(expr, HirBlockContext::Software)
    }

    fn from_syntax_with_context(expr: &Expr, context: HirBlockContext) -> Self {
        let span = expr.span();
        let node = match expr {
            Expr::Ident(name, _) => HirExprNode::Ident(name.clone()),
            Expr::Int(value, _) => HirExprNode::Int(*value),
            Expr::Str(value, _) => HirExprNode::Str(value.clone()),
            Expr::Bool(value, _) => HirExprNode::Bool(*value),
            Expr::Unary { op, expr, .. } => HirExprNode::Unary {
                op: *op,
                expr: Box::new(HirExpr::from_syntax_with_context(expr, context)),
            },
            Expr::Binary {
                op, left, right, ..
            } => HirExprNode::Binary {
                op: *op,
                left: Box::new(HirExpr::from_syntax_with_context(left, context)),
                right: Box::new(HirExpr::from_syntax_with_context(right, context)),
            },
            Expr::Call { callee, args, .. } => HirExprNode::Call {
                callee: Box::new(HirExpr::from_syntax_with_context(callee, context)),
                args: args
                    .iter()
                    .map(|arg| HirCallArg::from_syntax_with_context(arg, context))
                    .collect(),
            },
            Expr::GenericApp { callee, args, .. } => HirExprNode::GenericApp {
                callee: Box::new(HirExpr::from_syntax_with_context(callee, context)),
                args: args.iter().map(MirTypeRef::from).collect(),
            },
            Expr::Aggregate { ty, fields, .. } => HirExprNode::Aggregate {
                ty: Box::new(MirTypeRef::from(ty.as_ref())),
                fields: fields
                    .iter()
                    .map(|field| HirNamedExpr::from_syntax_with_context(field, context))
                    .collect(),
            },
            Expr::Field { base, field, .. } => HirExprNode::Field {
                base: Box::new(HirExpr::from_syntax_with_context(base, context)),
                field: field.clone(),
            },
            Expr::Index { base, index, .. } => HirExprNode::Index {
                base: Box::new(HirExpr::from_syntax_with_context(base, context)),
                index: Box::new(HirExpr::from_syntax_with_context(index, context)),
            },
            Expr::Group(expr, _) => {
                HirExprNode::Group(Box::new(HirExpr::from_syntax_with_context(expr, context)))
            }
            Expr::Block(block) => {
                HirExprNode::Block(HirBlock::from_syntax_with_context(block, context))
            }
            Expr::Match { expr, arms, .. } => HirExprNode::Match {
                expr: Box::new(HirExpr::from_syntax_with_context(expr, context)),
                arms: arms
                    .iter()
                    .map(|arm| HirMatchArm::from_syntax_with_context(arm, context))
                    .collect(),
            },
            Expr::Select { mode, arms, .. } => HirExprNode::Select {
                mode: MirSelectMode::from(*mode),
                arms: arms
                    .iter()
                    .map(|arm| HirSelectArm::from_syntax_with_context(arm, context))
                    .collect(),
            },
            Expr::Place {
                callee,
                args,
                inplace,
                ..
            } => HirExprNode::Place {
                callee: Box::new(HirExpr::from_syntax_with_context(callee, context)),
                args: args
                    .iter()
                    .map(|arg| HirCallArg::from_syntax_with_context(arg, context))
                    .collect(),
                inplace: *inplace,
            },
            Expr::For {
                name, range, body, ..
            } => HirExprNode::For {
                id: None,
                name: name.clone(),
                range: Box::new(HirExpr::from_syntax_with_context(range, context)),
                body: HirBlock::from_syntax_with_context(body, context),
            },
            Expr::CompileError { message, .. } => HirExprNode::CompileError {
                message: Box::new(HirExpr::from_syntax_with_context(message, context)),
            },
            Expr::Range { start, end, .. } => HirExprNode::Range {
                start: Box::new(HirExpr::from_syntax_with_context(start, context)),
                end: Box::new(HirExpr::from_syntax_with_context(end, context)),
            },
            _ => HirExprNode::Unsupported,
        };
        HirExpr {
            id: Self::unallocated_id(),
            node,
            span,
        }
    }

    /// Returns the expression's arena ID.
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

    /// Returns the source span of this expression.
    pub fn span(&self) -> Span {
        self.span
    }
}

/// The typed node of a HIR expression, analogous to AST `Expr` variants
/// but with resolved types, IDs, and lowering adjustments.
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
        inplace: bool,
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

/// A named (key-value) expression within an aggregate in the HIR.
#[derive(Clone)]
#[non_exhaustive]
pub struct HirNamedExpr {
    pub name: String,
    pub value: HirExpr,
}

impl HirNamedExpr {
    fn from_syntax_with_context(expr: &NamedExpr, context: HirBlockContext) -> Self {
        Self {
            name: expr.name.clone(),
            value: HirExpr::from_syntax_with_context(&expr.value, context),
        }
    }
}

/// A call argument in the HIR, optionally named.
#[derive(Clone)]
#[non_exhaustive]
pub struct HirCallArg {
    pub name: Option<String>,
    pub value: HirExpr,
    pub(crate) span: Span,
}

impl HirCallArg {
    fn from_syntax_with_context(arg: &CallArg, context: HirBlockContext) -> Self {
        Self {
            name: arg.name.clone(),
            value: HirExpr::from_syntax_with_context(&arg.value, context),
            span: arg.span,
        }
    }

    /// Returns the source span of this argument.
    pub fn span(&self) -> Span {
        self.span
    }
}

/// An arm in a `match` expression within the HIR.
#[derive(Clone)]
#[non_exhaustive]
pub struct HirMatchArm {
    pub doc: Option<String>,
    pub pattern: MirPattern,
    pub value: HirExpr,
}

impl HirMatchArm {
    fn from_syntax_with_context(arm: &MatchArm, context: HirBlockContext) -> Self {
        Self {
            doc: arm.doc.clone(),
            pattern: MirPattern::from(&arm.pattern),
            value: HirExpr::from_syntax_with_context(&arm.value, context),
        }
    }
}

/// An arm in a `select` expression within the HIR.
#[derive(Clone)]
#[non_exhaustive]
pub struct HirSelectArm {
    pub doc: Option<String>,
    pub pattern: HirExpr,
    pub value: HirExpr,
}

impl HirSelectArm {
    fn from_syntax_with_context(arm: &SelectArm, context: HirBlockContext) -> Self {
        Self {
            doc: arm.doc.clone(),
            pattern: HirExpr::from_syntax_with_context(&arm.pattern, context),
            value: HirExpr::from_syntax_with_context(&arm.value, context),
        }
    }
}
