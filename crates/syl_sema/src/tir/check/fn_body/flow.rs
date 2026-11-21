use super::TypePhaseChecker;
use crate::hir::{HirBlock, HirBodyExpr, HirExprNode, HirStmt};

impl TypePhaseChecker {
    pub(super) fn fn_expr_definitely_returns(expr: &HirBodyExpr) -> bool {
        match &expr.node {
            HirExprNode::Group(inner) => Self::fn_expr_definitely_returns(inner),
            HirExprNode::Block(block) => Self::fn_block_definitely_returns(block),
            HirExprNode::Match { arms, .. } => {
                !arms.is_empty()
                    && arms
                        .iter()
                        .all(|arm| Self::fn_expr_definitely_returns(&arm.value))
            }
            _ => false,
        }
    }

    fn fn_block_definitely_returns(block: &HirBlock) -> bool {
        block.stmts.iter().any(|stmt| match stmt {
            HirStmt::Return(..) => true,
            HirStmt::Expr(expr) => Self::fn_expr_definitely_returns(expr),
            HirStmt::ElabIf {
                then_block,
                else_block: Some(else_block),
                ..
            } => {
                Self::fn_block_definitely_returns(then_block)
                    && Self::fn_block_definitely_returns(else_block)
            }
            _ => false,
        }) || block
            .tail
            .as_deref()
            .is_some_and(Self::fn_expr_definitely_returns)
    }

    // An absent block value is known void, not an unresolved type. Keep this
    // separate from Unknown recovery so incomplete return paths are diagnosed.
    pub(super) fn fn_expr_has_void_tail_block(expr: &HirBodyExpr) -> bool {
        match &expr.node {
            HirExprNode::Group(inner) => Self::fn_expr_has_void_tail_block(inner),
            HirExprNode::Block(block) => {
                !Self::fn_block_definitely_returns(block)
                    && block
                        .tail
                        .as_deref()
                        .is_none_or(Self::fn_expr_has_void_tail_block)
            }
            HirExprNode::Match { arms, .. } => arms
                .iter()
                .any(|arm| Self::fn_expr_has_void_tail_block(&arm.value)),
            _ => false,
        }
    }
}
