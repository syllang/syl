use super::{TirDesign, TirGenericArg, TirType, TypePhaseChecker};
use crate::{
    CompileError, TirError,
    hir::{HirBodyExpr, HirDefKind, HirExprNode},
    hir_view::HirDesignViewExt,
};
use syl_hir::DefId;

#[derive(Clone)]
pub(crate) struct ExtensionMethodCall<'a> {
    pub(crate) method: DefId,
    _receiver: &'a HirBodyExpr,
    pub(crate) inferred_args: Vec<TirGenericArg>,
}

#[derive(Clone)]
pub(crate) struct LoweredExtensionMethodCall<'a> {
    pub(crate) method: DefId,
    pub(crate) receiver: &'a HirBodyExpr,
    pub(crate) inferred_args: Vec<TirGenericArg>,
}

impl TirDesign {
    pub(crate) fn extension_method_call<'a>(
        &self,
        owner: DefId,
        callee: &'a HirBodyExpr,
    ) -> Option<LoweredExtensionMethodCall<'a>> {
        let (receiver, method_name) = method_callee(callee)?;
        let receiver_ty = self.expr_type_for(owner, receiver)?;
        let receiver_def = receiver_ty.definition()?;
        let method = self
            .hir()
            .extension_methods_for(receiver_def, method_name)
            .iter()
            .copied()
            .find(|method| {
                extension_method_visible(self.hir(), owner, *method)
                    && matches!(
                        self.hir().def_kind(*method),
                        Some(HirDefKind::Map | HirDefKind::Fn)
                    )
            })?;
        Some(LoweredExtensionMethodCall {
            method,
            receiver,
            inferred_args: receiver_ty.generic_args().to_vec(),
        })
    }

    fn expr_type_for(&self, owner: DefId, expr: &HirBodyExpr) -> Option<&TirType> {
        let id = match &expr.node {
            HirExprNode::Ident(_) => self
                .hir()
                .expr_resolution(owner, expr)
                .ok()
                .flatten()
                .and_then(|resolution| match resolution {
                    crate::hir_resolve::HirResolution::Local(id) => {
                        self.binding_types.get(&super::BindingRef::Local(id)).copied()
                    }
                    crate::hir_resolve::HirResolution::Def(id) => {
                        self.binding_types.get(&super::BindingRef::Def(id)).copied()
                    }
                    _ => None,
                })?,
            _ => *self.expr_types.get(&expr.id())?,
        };
        self.type_table.get(id)
    }
}

impl TypePhaseChecker {
    pub(crate) fn extension_method_call<'a>(
        &self,
        owner: DefId,
        callee: &'a HirBodyExpr,
    ) -> Option<ExtensionMethodCall<'a>> {
        let (receiver, method_name) = method_callee(callee)?;
        let receiver_ty = self.infer_expr_type(owner, receiver);
        let receiver_def = receiver_ty.definition()?;
        let candidates = self
            .hir
            .extension_methods_for(receiver_def, method_name)
            .iter()
            .copied()
            .filter(|method| extension_method_visible(&self.hir, owner, *method))
            .filter(|method| {
                matches!(self.hir.def_kind(*method), Some(HirDefKind::Map | HirDefKind::Fn))
            })
            .collect::<Vec<_>>();
        let method = *candidates.first()?;
        Some(ExtensionMethodCall {
            method,
            _receiver: receiver,
            inferred_args: receiver_ty.generic_args().to_vec(),
        })
    }

    pub(crate) fn checked_extension_method_call<'a>(
        &self,
        owner: DefId,
        callee: &'a HirBodyExpr,
    ) -> Result<Option<ExtensionMethodCall<'a>>, CompileError> {
        let Some((receiver, method_name)) = method_callee(callee) else {
            return Ok(None);
        };
        let receiver_ty = self.infer_expr_type(owner, receiver);
        let Some(receiver_def) = receiver_ty.definition() else {
            return Ok(None);
        };
        let candidates = self
            .hir
            .extension_methods_for(receiver_def, method_name)
            .iter()
            .copied()
            .filter(|method| extension_method_visible(&self.hir, owner, *method))
            .filter(|method| {
                matches!(self.hir.def_kind(*method), Some(HirDefKind::Map | HirDefKind::Fn))
            })
            .collect::<Vec<_>>();
        match candidates.as_slice() {
            [] => Err(CompileError::lowering_at(
                TirError::UnknownMethod {
                    receiver: receiver_ty.label(),
                    method: method_name.to_string(),
                },
                callee.span(),
            )),
            [method] => Ok(Some(ExtensionMethodCall {
                method: *method,
                _receiver: receiver,
                inferred_args: receiver_ty.generic_args().to_vec(),
            })),
            _ => Err(CompileError::lowering_at(
                TirError::AmbiguousMethod {
                    receiver: receiver_ty.label(),
                    method: method_name.to_string(),
                    candidates: candidates
                        .iter()
                        .filter_map(|def| self.hir.def_name(*def))
                        .collect::<Vec<_>>()
                        .join(", "),
                },
                callee.span(),
            )),
        }
    }

}

fn method_callee(callee: &HirBodyExpr) -> Option<(&HirBodyExpr, &str)> {
    let mut current = callee;
    loop {
        match &current.node {
            HirExprNode::Field { base, field } => return Some((base, field.as_str())),
            HirExprNode::GenericApp { callee, .. } | HirExprNode::Group(callee) => {
                current = callee;
            }
            _ => return None,
        }
    }
}

fn extension_method_visible(hir: &crate::hir::HirDesign, owner: DefId, method: DefId) -> bool {
    let Some(owner_package) = hir.package_path_for_def(owner) else {
        return false;
    };
    let Some(method_package) = hir.package_path_for_def(method) else {
        return false;
    };
    owner_package == method_package
        || hir.imports.iter().any(|import| {
            import.package_path == owner_package && import.path == method_package.segments()
        })
}
