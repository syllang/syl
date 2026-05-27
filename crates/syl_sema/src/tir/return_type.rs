use super::{
    TypePhaseChecker,
    type_system::{TirConstTerm, TirConstTermResolver, TirGenericArg, TirType},
};
use crate::{CompileError, hir::HirBodyExpr, ir::mir::MirTypeRef};
use syl_hir::DefId;

#[non_exhaustive]
pub(super) struct MapReturnTypeResolver<'checker, 'callee> {
    checker: &'checker TypePhaseChecker,
    call_owner: DefId,
    map_def: DefId,
    callee: &'callee HirBodyExpr,
}

impl<'checker, 'callee> MapReturnTypeResolver<'checker, 'callee> {
    pub(super) fn new(
        checker: &'checker TypePhaseChecker,
        call_owner: DefId,
        map_def: DefId,
        callee: &'callee HirBodyExpr,
    ) -> Self {
        Self {
            checker,
            call_owner,
            map_def,
            callee,
        }
    }

    pub(super) fn resolve(&self) -> Option<TirType> {
        let map = self.checker.hir.maps.get(&self.map_def)?;
        let ret_ty = map.ret_ty.as_ref()?;
        let bindings = self.generic_bindings().ok()?;
        SubstitutingTypeResolver::new(self.checker, self.map_def, &bindings)
            .resolve_mir_type_ref(&ret_ty.ty)
            .ok()
    }

    fn generic_bindings(&self) -> Result<Vec<TirGenericBinding>, CompileError> {
        let Some(map) = self.checker.hir.maps.get(&self.map_def) else {
            return Ok(Vec::new());
        };
        let inferred_args = self
            .checker
            .extension_method_call(self.call_owner, self.callee)
            .filter(|call| call.method == self.map_def)
            .map(|call| call.inferred_args)
            .unwrap_or_default();
        let explicit_args = self.callee_generic_args();
        if explicit_args.is_none() && !inferred_args.is_empty() {
            return Ok(inferred_args
                .into_iter()
                .zip(&map.generics)
                .map(|(arg, generic)| TirGenericBinding {
                    name: generic.name.clone(),
                    arg,
                })
                .collect());
        }
        let Some(args) = explicit_args else {
            return Ok(Vec::new());
        };
        args.iter()
            .zip(&map.generics)
            .map(|(arg, generic)| {
                let arg = if generic
                    .kind
                    .as_ref()
                    .and_then(|kind| self.checker.mir_type_kind(kind))
                    .is_some()
                {
                    TirGenericArg::Const(
                        TirConstTermResolver::new(self.checker, self.call_owner)
                            .resolve_mir_type_ref(arg),
                    )
                } else {
                    TirGenericArg::Type(Box::new(
                        self.checker.type_from_mir_type_ref(self.call_owner, arg)?,
                    ))
                };
                Ok(TirGenericBinding {
                    name: generic.name.clone(),
                    arg,
                })
            })
            .collect()
    }

    fn callee_generic_args(&self) -> Option<&[MirTypeRef]> {
        let mut current = self.callee;
        loop {
            match &current.node {
                crate::hir::HirExprNode::GenericApp { args, .. } => return Some(args),
                crate::hir::HirExprNode::Group(inner) => current = inner,
                _ => return None,
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
struct TirGenericBinding {
    name: String,
    arg: TirGenericArg,
}

#[non_exhaustive]
struct SubstitutingTypeResolver<'checker, 'bindings> {
    checker: &'checker TypePhaseChecker,
    owner: DefId,
    bindings: &'bindings [TirGenericBinding],
}

impl<'checker, 'bindings> SubstitutingTypeResolver<'checker, 'bindings> {
    fn new(
        checker: &'checker TypePhaseChecker,
        owner: DefId,
        bindings: &'bindings [TirGenericBinding],
    ) -> Self {
        Self {
            checker,
            owner,
            bindings,
        }
    }

    fn resolve_mir_type_ref(&self, ty: &MirTypeRef) -> Result<TirType, CompileError> {
        if let Some(path) = ty.path()
            && path.len() == 1
        {
            return self
                .type_binding(&path[0])
                .map_or_else(|| self.checker.type_from_mir_type_ref(self.owner, ty), Ok);
        }
        if let Some(base) = ty.generic_base() {
            return self.resolve_mir_generic_type(base, ty.args().unwrap_or_default());
        }
        if let Some((len, elem)) = ty.array() {
            return Ok(TirType::Array {
                len: self.resolve_mir_const_expr(len),
                elem: Box::new(self.resolve_mir_type_ref(elem)?),
            });
        }
        if let Some((base, view)) = ty.view_select() {
            return Ok(TirType::View {
                base: Box::new(self.resolve_mir_type_ref(base)?),
                view: view.to_string(),
            });
        }
        self.checker.type_from_mir_type_ref(self.owner, ty)
    }

    fn resolve_mir_generic_type(
        &self,
        base: &MirTypeRef,
        args: &[MirTypeRef],
    ) -> Result<TirType, CompileError> {
        if matches!(base.type_name(), Some("Clock" | "Reset")) {
            let domain = args
                .first()
                .map(|arg| self.resolve_mir_type_ref(arg).map(Box::new))
                .transpose()?;
            return match base.type_name() {
                Some("Clock") => Ok(TirType::Clock { domain }),
                Some("Reset") => Ok(TirType::Reset { domain }),
                _ => Ok(TirType::Unknown),
            };
        }
        if matches!(base.type_name(), Some("UInt" | "Bits" | "SInt")) {
            let width = args.first().map_or(TirConstTerm::NatLiteral(1), |arg| {
                self.resolve_mir_type_const_arg(arg)
            });
            return match base.type_name() {
                Some("UInt") => Ok(TirType::UInt { width }),
                Some("Bits") => Ok(TirType::Bits { width }),
                Some("SInt") => Ok(TirType::SInt { width }),
                _ => Ok(TirType::Unknown),
            };
        }
        let base_ty = self.resolve_mir_type_ref(base)?;
        let args = args
            .iter()
            .enumerate()
            .map(|(index, arg)| self.resolve_mir_generic_arg(&base_ty, index, arg))
            .collect::<Result<Vec<_>, CompileError>>()?;
        Ok(base_ty.with_args(args))
    }

    fn resolve_mir_generic_arg(
        &self,
        base: &TirType,
        index: usize,
        arg: &MirTypeRef,
    ) -> Result<TirGenericArg, CompileError> {
        if self
            .checker
            .generic_param_expects_const(base.definition(), index)
        {
            Ok(TirGenericArg::Const(self.resolve_mir_type_const_arg(arg)))
        } else {
            Ok(TirGenericArg::Type(Box::new(
                self.resolve_mir_type_ref(arg)?,
            )))
        }
    }

    fn resolve_mir_type_const_arg(&self, ty: &MirTypeRef) -> TirConstTerm {
        if let Some(path) = ty.path()
            && path.len() == 1
        {
            return self.const_binding(&path[0]).unwrap_or_else(|| {
                TirConstTermResolver::new(self.checker, self.owner).resolve_mir_type_ref(ty)
            });
        }
        TirConstTermResolver::new(self.checker, self.owner).resolve_mir_type_ref(ty)
    }

    fn resolve_mir_const_expr(&self, expr: &crate::ir::mir::MirConstExpr) -> TirConstTerm {
        if let Some(name) = expr.ident() {
            return self.const_binding(name).unwrap_or_else(|| {
                TirConstTermResolver::new(self.checker, self.owner).resolve_mir_const_expr(expr)
            });
        }
        TirConstTermResolver::new(self.checker, self.owner).resolve_mir_const_expr(expr)
    }

    fn type_binding(&self, name: &str) -> Option<TirType> {
        self.bindings
            .iter()
            .find(|binding| binding.name == name)
            .and_then(|binding| match &binding.arg {
                TirGenericArg::Type(ty) => Some((**ty).clone()),
                TirGenericArg::Const(_) => None,
            })
    }

    fn const_binding(&self, name: &str) -> Option<TirConstTerm> {
        self.bindings
            .iter()
            .find(|binding| binding.name == name)
            .and_then(|binding| match &binding.arg {
                TirGenericArg::Const(term) => Some(term.clone()),
                TirGenericArg::Type(_) => None,
            })
    }
}
