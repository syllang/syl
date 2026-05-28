use crate::program::{ElabExpr, ElabExprNode, ElabProgram, ElabResolution};
use syl_hir::DefId;

#[derive(Clone, Copy)]
#[non_exhaustive]
pub(super) enum EirBuiltinIntrinsic {
    HighZ,
    Zero,
}

#[non_exhaustive]
pub(super) struct EirBuiltinResolver<'a> {
    program: &'a ElabProgram,
    owner: Option<DefId>,
}

impl<'a> EirBuiltinResolver<'a> {
    pub(super) fn new(program: &'a ElabProgram, owner: Option<DefId>) -> Self {
        Self { program, owner }
    }

    pub(super) fn resolve_call_callee(&self, callee: &ElabExpr) -> Option<EirBuiltinIntrinsic> {
        let root = self.callee_root(callee)?;
        if self.has_user_resolution(root) {
            return None;
        }
        let ElabExprNode::Ident(name) = &root.node else {
            return None;
        };
        self.resolve_name(name)
    }

    fn has_user_resolution(&self, root: &ElabExpr) -> bool {
        let Some(owner) = self.owner else {
            return false;
        };
        matches!(
            self.program.expr_resolution(owner, root),
            Some(ElabResolution::Def(_) | ElabResolution::Local(_))
        )
    }

    fn resolve_name(&self, name: &str) -> Option<EirBuiltinIntrinsic> {
        match name {
            "z" => Some(EirBuiltinIntrinsic::HighZ),
            "zero" => Some(EirBuiltinIntrinsic::Zero),
            _ => None,
        }
    }

    fn callee_root<'b>(&self, callee: &'b ElabExpr) -> Option<&'b ElabExpr> {
        let mut current = callee;
        loop {
            match &current.node {
                ElabExprNode::Ident(_) => return Some(current),
                ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }
}
