use super::*;
use crate::hir::resolve::HirResolution;

impl CapabilityChecker<'_> {
    pub(super) fn check_memory_args(
        &self,
        owner: DefId,
        callee: &HirBodyExpr,
        args: &[HirCallArg],
        context: ReadCheckContext<'_>,
    ) -> Result<bool, CompileError> {
        let HirExprNode::Ident(name) = &callee.node else {
            return Ok(false);
        };
        if !matches!(name.as_str(), "mem_write" | "mem_read_sync")
            || matches!(
                self.ctx.expr_resolution(owner, callee)?,
                Some(HirResolution::Def(_) | HirResolution::Local(_))
            )
        {
            return Ok(false);
        }
        for (index, arg) in args.iter().enumerate() {
            if name == "mem_read_sync" && index == 4 {
                self.require_write(owner, &arg.value, context.scope, context.facts)?;
            } else {
                self.check_read_expr(owner, &arg.value, context)?;
            }
        }
        Ok(true)
    }
}
