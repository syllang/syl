use crate::{
    CompileError, ConstEvalError,
    const_mir::ConstExpr,
    eir::{EirBinaryOp, EirExpr, EirParamBind, EirUnaryOp},
    map_ir::MapGenericArg,
    program::{
        ElabCallArg, ElabCallable, ElabDefKind, ElabExpr, ElabExprNode, ElabResolution,
        ElabSignatureResultBinding,
    },
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use syl_hir::DefId;
use syl_sema::ir::const_mir::{ConstExprKind, ConstStmt, Terminator};

use super::{
    EirBuilder, Env,
    builtins::{EirBuiltinIntrinsic, EirBuiltinResolver},
};

#[derive(Clone)]
#[non_exhaustive]
enum SymbolicConstValue {
    Scalar(EirExpr),
    Struct(BTreeMap<String, SymbolicConstValue>),
}

impl SymbolicConstValue {
    fn into_scalar(self) -> Option<EirExpr> {
        match self {
            Self::Scalar(expr) => Some(expr),
            Self::Struct(_) => None,
        }
    }

    fn field(&self, field: &str) -> Option<Self> {
        match self {
            Self::Scalar(_) => None,
            Self::Struct(fields) => fields.get(field).cloned(),
        }
    }
}

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(in crate::eir::build) fn symbolic_const_condition_expr(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<EirExpr, CompileError> {
        let const_env = self.const_eval_env(env);
        let lowered = self
            .const_elaborator
            .lower_expr(self.program, expr, &const_env)?;
        self.symbolic_const_value(&lowered, env, &BTreeMap::new())
            .and_then(SymbolicConstValue::into_scalar)
            .ok_or_else(|| {
                CompileError::lowering_at(
                    crate::EirError::InvalidElaborationExpression,
                    expr.span(),
                )
            })
    }

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

    fn symbolic_const_value(
        &self,
        expr: &ConstExpr,
        env: &Env,
        locals: &BTreeMap<String, SymbolicConstValue>,
    ) -> Option<SymbolicConstValue> {
        match expr.kind() {
            ConstExprKind::Local(local) => locals
                .get(local.name())
                .cloned()
                .or_else(|| self.symbolic_named_local(local.name(), env)),
            ConstExprKind::Unknown(_) => None,
            ConstExprKind::Nat(value) => Some(SymbolicConstValue::Scalar(EirExpr::Int(*value))),
            ConstExprKind::Bool(value) => Some(SymbolicConstValue::Scalar(EirExpr::Bool(*value))),
            ConstExprKind::Aggregate { fields, .. } => Some(SymbolicConstValue::Struct(
                fields
                    .iter()
                    .map(|field| {
                        Some((
                            field.name().to_string(),
                            self.symbolic_const_value(field.value(), env, locals)?,
                        ))
                    })
                    .collect::<Option<BTreeMap<_, _>>>()?,
            )),
            ConstExprKind::Field { base, field } => {
                self.symbolic_const_value(base, env, locals)?.field(field)
            }
            ConstExprKind::Unary { op, expr } => Some(SymbolicConstValue::Scalar(EirExpr::unary(
                self.symbolic_unary_op(*op)?,
                self.symbolic_const_value(expr, env, locals)?
                    .into_scalar()?,
            ))),
            ConstExprKind::Binary { op, left, right } => {
                Some(SymbolicConstValue::Scalar(EirExpr::binary(
                    self.symbolic_binary_op(*op)?,
                    self.symbolic_const_value(left, env, locals)?
                        .into_scalar()?,
                    self.symbolic_const_value(right, env, locals)?
                        .into_scalar()?,
                )))
            }
            ConstExprKind::Call { callee, args } => {
                self.symbolic_const_call(*callee, args, env, locals)
            }
            ConstExprKind::Unsupported => None,
            _ => None,
        }
    }

    fn symbolic_const_call(
        &self,
        callee: DefId,
        args: &[ConstExpr],
        env: &Env,
        locals: &BTreeMap<String, SymbolicConstValue>,
    ) -> Option<SymbolicConstValue> {
        let function = self.const_elaborator.function(callee)?;
        let mut bindings = function
            .params()
            .iter()
            .zip(args)
            .map(|(param, arg)| Some((param.clone(), self.symbolic_const_value(arg, env, locals)?)))
            .collect::<Option<BTreeMap<_, _>>>()?;
        let mut current = function.entry();
        let mut visited = BTreeSet::new();
        loop {
            if !visited.insert(current) {
                return None;
            }
            let block = function.block(current)?;
            for stmt in block.stmts() {
                match stmt {
                    ConstStmt::Assign { local, value } => {
                        bindings.insert(
                            local.name().to_string(),
                            self.symbolic_const_value(value, env, &bindings)?,
                        );
                    }
                    _ => return None,
                }
            }
            match block.terminator() {
                Terminator::Goto(target) => current = *target,
                Terminator::Return(Some(value)) => {
                    return self.symbolic_const_value(value, env, &bindings);
                }
                Terminator::Branch { .. } | Terminator::Return(None) => return None,
                _ => return None,
            }
        }
    }

    fn symbolic_named_local(&self, name: &str, env: &Env) -> Option<SymbolicConstValue> {
        let var = env.var(name)?;
        if var.software_local {
            let ty = self.canonicalize_const_eval_type(env.owner, &var.ty);
            if matches!(
                self.const_elaborator.kind_for_type(&ty),
                Some(crate::const_eval::ConstKind::Struct(_))
            ) && let Some(value) = self.symbolic_software_local_struct(name, env)
            {
                return Some(value);
            }
        }
        Some(SymbolicConstValue::Scalar(var.code.clone()))
    }

    fn symbolic_software_local_struct(&self, root: &str, env: &Env) -> Option<SymbolicConstValue> {
        let prefix = format!("{root}.");
        let field_names = env
            .vars
            .iter()
            .filter(|(name, var)| var.software_local && name.starts_with(&prefix))
            .filter_map(|(name, _)| name[prefix.len()..].split('.').next().map(str::to_string))
            .collect::<BTreeSet<_>>();
        if field_names.is_empty() {
            return None;
        }
        Some(SymbolicConstValue::Struct(
            field_names
                .into_iter()
                .map(|field_name| {
                    let field_key = format!("{root}.{field_name}");
                    Some((field_name, self.symbolic_named_local(&field_key, env)?))
                })
                .collect::<Option<BTreeMap<_, _>>>()?,
        ))
    }

    fn symbolic_unary_op(&self, op: crate::mir::MirUnaryOp) -> Option<EirUnaryOp> {
        match op {
            crate::mir::MirUnaryOp::Not | crate::mir::MirUnaryOp::NotWord => Some(EirUnaryOp::Not),
            crate::mir::MirUnaryOp::Neg => Some(EirUnaryOp::Neg),
            _ => None,
        }
    }

    fn symbolic_binary_op(&self, op: crate::mir::MirBinaryOp) -> Option<EirBinaryOp> {
        match op {
            crate::mir::MirBinaryOp::OrOr => Some(EirBinaryOp::OrOr),
            crate::mir::MirBinaryOp::AndAnd => Some(EirBinaryOp::AndAnd),
            crate::mir::MirBinaryOp::Eq => Some(EirBinaryOp::Eq),
            crate::mir::MirBinaryOp::NotEq => Some(EirBinaryOp::NotEq),
            crate::mir::MirBinaryOp::Lt => Some(EirBinaryOp::Lt),
            crate::mir::MirBinaryOp::LtEq => Some(EirBinaryOp::LtEq),
            crate::mir::MirBinaryOp::Gt => Some(EirBinaryOp::Gt),
            crate::mir::MirBinaryOp::GtEq => Some(EirBinaryOp::GtEq),
            crate::mir::MirBinaryOp::Add => Some(EirBinaryOp::Add),
            crate::mir::MirBinaryOp::Sub => Some(EirBinaryOp::Sub),
            crate::mir::MirBinaryOp::Mul => Some(EirBinaryOp::Mul),
            crate::mir::MirBinaryOp::Div => Some(EirBinaryOp::Div),
            crate::mir::MirBinaryOp::Rem => Some(EirBinaryOp::Rem),
            crate::mir::MirBinaryOp::Shl => Some(EirBinaryOp::Shl),
            crate::mir::MirBinaryOp::BitAnd => Some(EirBinaryOp::BitAnd),
            crate::mir::MirBinaryOp::BitOr => Some(EirBinaryOp::BitOr),
            crate::mir::MirBinaryOp::BitXor => Some(EirBinaryOp::BitXor),
            _ => None,
        }
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
        env: &Env,
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
                    let owner = env.owner.or(Some(def));
                    if generic.kind.is_none() {
                        let arg = self.canonicalize_callsite_type(owner, arg);
                        out.push(EirParamBind::new(
                            format!("{}_WIDTH", generic.name),
                            self.width(owner, &arg),
                        ));
                    } else {
                        let arg = self.canonicalize_callsite_type(owner, arg);
                        out.push(EirParamBind::new(
                            &generic.name,
                            self.type_value_in_env(owner, &arg, env),
                        ));
                    }
                }
            }
        }
        out
    }

    fn type_value_in_env(&self, owner: Option<DefId>, ty: &crate::mir::MirTypeRef, env: &Env) -> String {
        if let Some(name) = ty.path_name()
            && let Some(var) = env.var(name)
            && !var.software_local
            && let Some(value) = self.const_value_for_var(name, var, env)
        {
            return match value {
                crate::const_eval::ConstValue::Nat(value) => value.to_string(),
                crate::const_eval::ConstValue::Bool(value) => value.to_string(),
                crate::const_eval::ConstValue::Unknown(_) => name.to_string(),
                _ => name.to_string(),
            };
        }
        self.type_value(owner, ty)
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
