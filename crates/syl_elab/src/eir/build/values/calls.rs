use crate::{
    CompileError, ConstEvalError,
    eir::{EirExpr, EirParamBind},
    map_ir::MapGenericArg,
    program::{
        ElabCallArg, ElabCallable, ElabDefKind, ElabExpr, ElabExprNode, ElabResolution,
        ElabSignatureResultBinding,
    },
};
use std::collections::HashMap;
use syl_hir::DefId;

use super::{
    EirBuilder, Env,
    builtins::{EirBuiltinIntrinsic, EirBuiltinResolver},
};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn elab_call_expr(
        &self,
        callee: &ElabExpr,
        args: &[ElabCallArg],
        env: &Env,
    ) -> EirExpr {
        match EirBuiltinResolver::new(self.program, env.owner).resolve_call_callee(callee) {
            Some(EirBuiltinIntrinsic::HighZ) => return EirExpr::high_z(),
            Some(EirBuiltinIntrinsic::Zero) => return EirExpr::zero(),
            _ => {}
        }
        if self.map_callee_from_elab(callee, env).is_some() {
            return self.map_call_expr_from_elab(callee, args, env);
        }
        if let Some(expr) = self.extension_map_call_expr(callee, args, env) {
            return expr;
        }
        let Some(name) = self.elab_expr_name(callee) else {
            return EirExpr::unsupported("call callee is not a name");
        };
        EirExpr::call(
            name,
            args.iter()
                .map(|arg| self.elab_expr(&arg.value, env))
                .collect(),
        )
    }

    pub(in crate::eir::build) fn elab_expr_name(&self, expr: &ElabExpr) -> Option<String> {
        let mut current = expr;
        loop {
            match &current.node {
                ElabExprNode::Ident(name) => return Some(name.clone()),
                ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                    current = callee;
                }
                _ => return None,
            }
        }
    }

    pub(in crate::eir::build) fn elab_callee_root<'b>(
        &self,
        expr: &'b ElabExpr,
    ) -> Option<&'b ElabExpr> {
        let mut current = expr;
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

    pub(in crate::eir::build) fn generic_actuals_for_elab(
        &self,
        def: DefId,
        callee: &ElabExpr,
        owner: Option<DefId>,
    ) -> Vec<EirParamBind> {
        let ElabExprNode::GenericApp { args, .. } = &callee.node else {
            return Vec::new();
        };
        let mut out = Vec::new();
        if let Some(callable) = self.program.callable_by_def(def) {
            for (idx, generic) in callable.generics().iter().enumerate() {
                if self.is_domain_param(generic) {
                    continue;
                }
                if let Some(arg) = args.get(idx) {
                    if generic.kind.is_none() {
                        let arg = self.canonicalize_callsite_type(owner, arg);
                        out.push(EirParamBind::new(
                            format!("{}_WIDTH", generic.name),
                            self.width(owner.or(Some(def)), &arg),
                        ));
                    } else {
                        let arg = self.canonicalize_callsite_type(owner, arg);
                        out.push(EirParamBind::new(
                            &generic.name,
                            self.type_value(owner.or(Some(def)), &arg),
                        ));
                    }
                }
            }
        }
        out
    }

    pub(in crate::eir::build) fn callable_result_for(
        &self,
        def: DefId,
    ) -> Option<&ElabSignatureResultBinding> {
        self.program.callable_by_def(def)?.result()
    }

    pub(in crate::eir::build) fn callable_params_for_elab(
        &self,
        def: DefId,
        callable: &str,
        callee: &ElabExpr,
    ) -> Result<Vec<(String, crate::mir::MirTypeRef)>, CompileError> {
        let Some(callable_ref) = self.program.callable_by_def(def) else {
            return Err(CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier {
                    name: callable.to_string(),
                },
                callee.span(),
            ));
        };
        Ok(callable_ref
            .params()
            .iter()
            .map(|param| {
                let ty = param.ty.clone();
                (
                    param.name.clone(),
                    self.specialize_type_for_elab(&ty, def, callee),
                )
            })
            .collect())
    }

    pub(in crate::eir::build) fn callable_result_type_from_elab(
        &self,
        callee: &ElabExpr,
        env: &Env,
    ) -> Option<crate::mir::MirTypeRef> {
        let def = self.callee_def_from_elab(callee, env)?;
        self.callable_result_type_for_elab_def(def, callee)
    }

    pub(in crate::eir::build) fn callable_result_type_for_elab_def(
        &self,
        def: DefId,
        callee: &ElabExpr,
    ) -> Option<crate::mir::MirTypeRef> {
        let result = self.callable_result_for(def)?;
        Some(self.specialize_type_for_elab(&result.ty, def, callee))
    }

    pub(in crate::eir::build) fn specialize_type_for_elab(
        &self,
        ty: &crate::mir::MirTypeRef,
        def: DefId,
        callee: &ElabExpr,
    ) -> crate::mir::MirTypeRef {
        let ElabExprNode::GenericApp { args, .. } = &callee.node else {
            return ty.clone();
        };
        let Some(callable) = self.program.callable_by_def(def) else {
            return ty.clone();
        };
        let mut replacements = HashMap::new();
        for (idx, generic) in callable.generics().iter().enumerate() {
            if let Some(arg) = args.get(idx) {
                replacements.insert(generic.name.clone(), arg.clone());
            }
        }
        self.subst_type_vars(ty, &replacements)
    }

    pub(in crate::eir::build) fn callable_from_elab<'b>(
        &'b self,
        callee: &ElabExpr,
        env: &Env,
    ) -> Result<(DefId, String, &'b ElabCallable), CompileError> {
        let name = self.elab_expr_name(callee).ok_or_else(|| {
            CompileError::lowering_at(crate::EirError::InstanceCalleeMustBeName, callee.span())
        })?;
        let def = env
            .owner
            .and_then(|owner| self.resolved_elab_callee_def(owner, callee))
            .ok_or_else(|| {
                CompileError::lowering_at(
                    ConstEvalError::UnknownElaborationIdentifier { name: name.clone() },
                    callee.span(),
                )
            })?;
        let callable = self.program.callable_by_def(def).ok_or_else(|| {
            CompileError::lowering_at(
                ConstEvalError::UnknownElaborationIdentifier { name: name.clone() },
                callee.span(),
            )
        })?;
        let canonical_name = self.program.def_name(def).unwrap_or(&name).to_string();
        Ok((def, canonical_name, callable))
    }

    pub(in crate::eir::build) fn callee_def_from_elab(
        &self,
        callee: &ElabExpr,
        env: &Env,
    ) -> Option<DefId> {
        let owner = env.owner?;
        self.resolved_elab_callee_def(owner, callee)
    }

    fn resolved_elab_callee_def(&self, owner: DefId, callee: &ElabExpr) -> Option<DefId> {
        let root = self.elab_callee_root(callee)?;
        let Some(ElabResolution::Def(def)) = self.program.expr_resolution(owner, root) else {
            return None;
        };
        Some(def)
    }

    pub(in crate::eir::build) fn extension_map_call_expr(
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

    fn method_callee<'b>(&self, callee: &'b ElabExpr) -> Option<(&'b ElabExpr, &'b str)> {
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
        if let Some(ty) = self.program.expr_type(owner, receiver) {
            return Some(ty);
        }
        match &receiver.node {
            ElabExprNode::GenericApp { callee, .. } | ElabExprNode::Group(callee) => {
                self.receiver_type(owner, callee)
            }
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
