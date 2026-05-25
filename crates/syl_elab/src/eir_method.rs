use crate::{
    eir_build::{EirBuilder, Env},
    eir_expr::EirExpr,
    map_ir::MapGenericArg,
    program::{ElabCallArg, ElabDefKind, ElabExpr, ElabExprNode, ElabResolution},
};
use syl_hir::DefId;

impl<'a> EirBuilder<'a> {
    pub(super) fn extension_map_call_expr(
        &self,
        callee: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> Option<EirExpr> {
        let owner = env.owner?;
        let (receiver, method_name) = self.method_callee(callee)?;
        let receiver_ty = self.receiver_type(owner, receiver)?;
        let receiver_def = receiver_ty.definition()?;
        let method = self
            .program
            .extension_methods_for(receiver_def, method_name)
            .iter()
            .copied()
            .find(|method| self.program.def_kind(*method) == Some(ElabDefKind::Map))?;
        let mut call_args = vec![ElabCallArg {
            name: None,
            value: receiver.clone(),
            span: receiver.span(),
        }];
        call_args.extend(args.iter().cloned());
        let explicit_generics = self.elab_generic_type_args(callee);
        let inferred_generics = receiver_ty
            .generic_args()
            .iter()
            .map(MapGenericArg::from)
            .collect::<Vec<_>>();
        let generics = if explicit_generics.is_empty() {
            inferred_generics.as_slice()
        } else {
            explicit_generics.as_slice()
        };
        Some(self.map_extension_call_expr(method, generics, &call_args, env))
    }

    pub(super) fn method_callee<'b>(
        &self,
        callee: &'b ElabExpr,
    ) -> Option<(&'b ElabExpr, &'b str)> {
        let mut current = callee;
        loop {
            match &current.node {
                ElabExprNode::Field { base, field } => return Some((base, field.as_str())),
                ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }

    fn receiver_type(&self, owner: DefId, receiver: &ElabExpr) -> Option<&crate::tir::TirType> {
        match &receiver.node {
            ElabExprNode::Ident(_) => {
                let ElabResolution::Local(local) = self.program.expr_resolution(owner, receiver)?
                else {
                    return None;
                };
                self.program.local_type(local)
            }
            _ => None,
        }
    }
}
