use super::{TirDesign, TirGenericArg, TirType, TypePhaseChecker};
use crate::{
    CompileError, TirError,
    hir::view::HirDesignViewExt,
    hir::{HirBodyExpr, HirDefKind, HirExprNode},
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

enum ExtensionMethodResolution {
    Missing,
    One(DefId),
    Ambiguous(Vec<DefId>),
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
        let ExtensionMethodResolution::One(method) =
            resolve_extension_method(self.hir(), owner, receiver_def, method_name)
        else {
            return None;
        };
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
                    crate::hir::resolve::HirResolution::Local(id) => self
                        .binding_types
                        .get(&super::BindingRef::Local(id))
                        .copied(),
                    crate::hir::resolve::HirResolution::Def(id) => {
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
        match resolve_extension_method(&self.hir, owner, receiver_def, method_name) {
            ExtensionMethodResolution::One(method) => Some(ExtensionMethodCall {
                method,
                _receiver: receiver,
                inferred_args: receiver_ty.generic_args().to_vec(),
            }),
            ExtensionMethodResolution::Missing | ExtensionMethodResolution::Ambiguous(_) => None,
        }
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
        match resolve_extension_method(&self.hir, owner, receiver_def, method_name) {
            ExtensionMethodResolution::Missing => Err(CompileError::lowering_at(
                TirError::UnknownMethod {
                    receiver: receiver_ty.label(),
                    method: method_name.to_string(),
                },
                callee.span(),
            )),
            ExtensionMethodResolution::One(method) => Ok(Some(ExtensionMethodCall {
                method,
                _receiver: receiver,
                inferred_args: receiver_ty.generic_args().to_vec(),
            })),
            ExtensionMethodResolution::Ambiguous(candidates) => Err(CompileError::lowering_at(
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

fn resolve_extension_method(
    hir: &crate::hir::HirDesign,
    owner: DefId,
    receiver_def: DefId,
    method_name: &str,
) -> ExtensionMethodResolution {
    let candidates = hir
        .extension_methods_for(receiver_def, method_name)
        .iter()
        .copied()
        .filter(|method| extension_method_visible(hir, owner, *method))
        .filter(|method| {
            matches!(
                hir.def_kind(*method),
                Some(HirDefKind::Map | HirDefKind::Fn)
            )
        })
        .collect::<Vec<_>>();
    match candidates.as_slice() {
        [] => ExtensionMethodResolution::Missing,
        [method] => ExtensionMethodResolution::One(*method),
        _ => ExtensionMethodResolution::Ambiguous(candidates),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hir::{HirDef, HirDefKind, HirDesign};
    use syl_hir::HirPath;
    use syl_span::Span;

    #[test]
    fn resolver_reports_ambiguous_extension_methods() {
        let receiver = DefId::new(0);
        let first = DefId::new(1);
        let second = DefId::new(2);
        let owner = DefId::new(3);
        let mut hir = HirDesign::empty();
        hir.defs = vec![
            def(receiver, "Word", "pkg.Word", HirDefKind::Bundle),
            def(first, "flag", "pkg.flag_a", HirDefKind::Map),
            def(second, "flag", "pkg.flag_b", HirDefKind::Map),
            def(owner, "Top", "pkg.Top", HirDefKind::Cell),
        ];
        hir.register_extension_method(receiver, "flag".to_string(), first);
        hir.register_extension_method(receiver, "flag".to_string(), second);

        match resolve_extension_method(&hir, owner, receiver, "flag") {
            ExtensionMethodResolution::Ambiguous(candidates) => {
                assert_eq!(candidates, vec![first, second]);
            }
            _ => panic!("resolver must preserve ambiguous extension method candidates"),
        }
    }

    fn def(id: DefId, name: &str, path: &str, kind: HirDefKind) -> HirDef {
        HirDef::new(
            id,
            name.to_string(),
            HirPath::new(path.split('.').map(str::to_string).collect()),
            kind,
            Span::default(),
        )
    }
}
