use crate::hir::HirDesign;
use crate::hir::resolve::HirResolution;
use crate::hir::view::HirDesignViewExt;
use crate::hir::{HirBodyExpr, HirExprNode};
use syl_hir::DefId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum BuiltinIntrinsic {
    HighZ,
    Zero,
}

#[non_exhaustive]
pub struct BuiltinResolver<'a> {
    hir: &'a HirDesign,
    owner: Option<DefId>,
}

impl<'a> BuiltinResolver<'a> {
    pub fn new(hir: &'a HirDesign, owner: Option<DefId>) -> Self {
        Self { hir, owner }
    }

    pub fn resolve_call_callee(&self, callee: &HirBodyExpr) -> Option<BuiltinIntrinsic> {
        let root = self.callee_root(callee)?;
        if self.has_user_resolution(root) {
            return None;
        }
        let HirExprNode::Ident(name) = &root.node else {
            return None;
        };
        self.resolve_name(name)
    }

    fn has_user_resolution(&self, root: &HirBodyExpr) -> bool {
        let Some(owner) = self.owner else {
            return false;
        };
        matches!(
            self.hir.expr_resolution(owner, root),
            Ok(Some(HirResolution::Def(_) | HirResolution::Local(_)))
        )
    }

    fn resolve_name(&self, name: &str) -> Option<BuiltinIntrinsic> {
        match name {
            "z" => Some(BuiltinIntrinsic::HighZ),
            "zero" => Some(BuiltinIntrinsic::Zero),
            _ => None,
        }
    }

    fn callee_root<'b>(&self, callee: &'b HirBodyExpr) -> Option<&'b HirBodyExpr> {
        let mut current = callee;
        loop {
            match &current.node {
                HirExprNode::Ident(_) => return Some(current),
                HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }
}
