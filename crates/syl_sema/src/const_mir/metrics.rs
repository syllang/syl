use super::{
    BasicBlock, ConstExpr, ConstExprKind, ConstFunction, ConstLocal, ConstLocalRef,
    ConstMirProgram, ConstStmt, Terminator,
};
use syl_hir::LocalId;

impl ConstMirProgram {
    pub fn node_count(&self) -> usize {
        self.functions.iter().map(ConstFunction::node_count).sum()
    }

    pub fn local_ref_count(&self) -> usize {
        self.functions
            .iter()
            .map(ConstFunction::local_ref_count)
            .sum()
    }

    pub fn resolved_local_ref_count(&self) -> usize {
        self.functions
            .iter()
            .map(ConstFunction::resolved_local_ref_count)
            .sum()
    }
}

impl ConstFunction {
    fn node_count(&self) -> usize {
        self.name.len()
            + self.params.iter().map(String::len).sum::<usize>()
            + usize::from(self.unsupported)
            + self
                .locals
                .iter()
                .map(ConstLocal::node_count)
                .sum::<usize>()
            + self
                .blocks
                .iter()
                .map(BasicBlock::node_count)
                .sum::<usize>()
            + self.span.start
    }

    fn local_ref_count(&self) -> usize {
        self.blocks.iter().map(BasicBlock::local_ref_count).sum()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.blocks
            .iter()
            .map(BasicBlock::resolved_local_ref_count)
            .sum()
    }
}

impl ConstLocal {
    fn node_count(&self) -> usize {
        self.name.len() + self.id.map(LocalId::get).unwrap_or_default()
    }
}

impl ConstLocalRef {
    fn node_count(&self) -> usize {
        self.name.len() + self.id.map(LocalId::get).unwrap_or_default()
    }
}

impl BasicBlock {
    fn node_count(&self) -> usize {
        self.id.index
            + self.stmts.iter().map(ConstStmt::node_count).sum::<usize>()
            + self.term.node_count()
    }

    fn local_ref_count(&self) -> usize {
        self.stmts
            .iter()
            .map(ConstStmt::local_ref_count)
            .sum::<usize>()
            + self.term.local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.stmts
            .iter()
            .map(ConstStmt::resolved_local_ref_count)
            .sum::<usize>()
            + self.term.resolved_local_ref_count()
    }
}

impl ConstStmt {
    fn node_count(&self) -> usize {
        match self {
            Self::Assign { local, value } => local.node_count() + value.node_count(),
        }
    }

    fn local_ref_count(&self) -> usize {
        match self {
            Self::Assign { value, .. } => 1 + value.local_ref_count(),
        }
    }

    fn resolved_local_ref_count(&self) -> usize {
        match self {
            Self::Assign { local, value } => {
                usize::from(local.id().is_some()) + value.resolved_local_ref_count()
            }
        }
    }
}

impl Terminator {
    fn node_count(&self) -> usize {
        match self {
            Self::Goto(target) => target.index,
            Self::Branch {
                cond,
                then_block,
                else_block,
            } => cond.node_count() + then_block.index + else_block.index,
            Self::Return(value) => value.as_ref().map(ConstExpr::node_count).unwrap_or(1),
        }
    }

    fn local_ref_count(&self) -> usize {
        match self {
            Self::Goto(_) => 0,
            Self::Branch { cond, .. } => cond.local_ref_count(),
            Self::Return(value) => value.as_ref().map(ConstExpr::local_ref_count).unwrap_or(0),
        }
    }

    fn resolved_local_ref_count(&self) -> usize {
        match self {
            Self::Goto(_) => 0,
            Self::Branch { cond, .. } => cond.resolved_local_ref_count(),
            Self::Return(value) => value
                .as_ref()
                .map(ConstExpr::resolved_local_ref_count)
                .unwrap_or(0),
        }
    }
}

impl ConstExpr {
    fn node_count(&self) -> usize {
        self.span.start + self.kind.node_count()
    }

    fn local_ref_count(&self) -> usize {
        self.kind.local_ref_count()
    }

    fn resolved_local_ref_count(&self) -> usize {
        self.kind.resolved_local_ref_count()
    }
}

impl ConstExprKind {
    fn node_count(&self) -> usize {
        match self {
            Self::Local(local) => local.node_count(),
            Self::Unknown(_) => 1,
            Self::Nat(value) => usize::from(*value != 0),
            Self::Bool(value) => usize::from(*value),
            Self::Unary { expr, .. } => 1 + expr.node_count(),
            Self::Binary { left, right, .. } => 1 + left.node_count() + right.node_count(),
            Self::Call { callee, args } => {
                callee.get() + args.iter().map(ConstExpr::node_count).sum::<usize>()
            }
            Self::Unsupported => 1,
        }
    }

    fn local_ref_count(&self) -> usize {
        match self {
            Self::Local(_) => 1,
            Self::Unary { expr, .. } => expr.local_ref_count(),
            Self::Binary { left, right, .. } => left.local_ref_count() + right.local_ref_count(),
            Self::Call { args, .. } => args.iter().map(ConstExpr::local_ref_count).sum(),
            Self::Unknown(_) | Self::Nat(_) | Self::Bool(_) | Self::Unsupported => 0,
        }
    }

    fn resolved_local_ref_count(&self) -> usize {
        match self {
            Self::Local(local) => usize::from(local.id().is_some()),
            Self::Unary { expr, .. } => expr.resolved_local_ref_count(),
            Self::Binary { left, right, .. } => {
                left.resolved_local_ref_count() + right.resolved_local_ref_count()
            }
            Self::Call { args, .. } => args.iter().map(ConstExpr::resolved_local_ref_count).sum(),
            Self::Unknown(_) | Self::Nat(_) | Self::Bool(_) | Self::Unsupported => 0,
        }
    }
}
