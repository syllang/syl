use super::super::{TirGenericArg, TirType, TypePhaseChecker};
use crate::{
    CompileError, TirError,
    hir::{
        HirConstItem, HirFieldDecl, HirSignatureGenericParam, HirStructItem, resolve::HirResolution,
    },
};
use std::collections::{BTreeMap, BTreeSet};
use syl_hir::{DefId, LocalId};

const MAX_TYPE_EXPANSION_DEPTH: usize = 64;
type StructExpansionKey = (DefId, Vec<TirGenericArg>);

impl TypePhaseChecker {
    pub(super) fn check_const_cycle(
        &self,
        owner: DefId,
        item: &HirConstItem,
        errors: &mut Vec<CompileError>,
    ) {
        if self.const_dependency_cycle(owner, &mut BTreeSet::new(), &mut BTreeSet::new()) {
            errors.push(CompileError::lowering_at(
                TirError::ConstCycle {
                    name: item.name.clone(),
                },
                item.value.span(),
            ));
        }
    }

    fn const_dependency_cycle(
        &self,
        def: DefId,
        active: &mut BTreeSet<DefId>,
        done: &mut BTreeSet<DefId>,
    ) -> bool {
        if done.contains(&def) {
            return false;
        }
        if !active.insert(def) {
            return true;
        }
        for expr in self.hir.exprs().iter().filter(|expr| expr.owner == def) {
            if let Some(HirResolution::Def(dep)) = self.hir.expr_resolutions().get(&expr.id)
                && self.hir.consts().contains_key(dep)
                && self.const_dependency_cycle(*dep, active, done)
            {
                return true;
            }
        }
        active.remove(&def);
        done.insert(def);
        false
    }

    pub(in crate::tir) fn check_struct(
        &mut self,
        owner: DefId,
        item: &HirStructItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.check_generics(owner, &item.generics, errors)?;
        for field in &item.fields {
            Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &field.ty));
        }
        self.check_value_type_recursion(
            owner,
            &item.fields,
            TirError::RecursiveStruct {
                name: item.name.clone(),
            },
            errors,
        );
        Ok(())
    }

    pub(super) fn check_value_type_recursion(
        &self,
        owner: DefId,
        fields: &[HirFieldDecl],
        error: TirError,
        errors: &mut Vec<CompileError>,
    ) {
        for field in fields {
            let Ok(ty) = self.type_from_mir_type_ref(owner, &field.ty) else {
                continue;
            };
            let mut stack = vec![(owner, Vec::new())];
            if self.recursive_value_type(&ty, &mut stack) {
                errors.push(CompileError::lowering_at(error.clone(), field.span));
            }
        }
    }

    fn recursive_value_type(&self, ty: &TirType, stack: &mut Vec<StructExpansionKey>) -> bool {
        match ty {
            TirType::Array { elem, .. } | TirType::View { base: elem, .. } => {
                self.recursive_value_type(elem, stack)
            }
            TirType::Named {
                def: Some(def),
                args,
                ..
            } => {
                let Some((fields, generics)) = self.value_type_fields(*def) else {
                    return false;
                };
                if stack.len() >= MAX_TYPE_EXPANSION_DEPTH
                    || stack.first().is_some_and(|(root, _)| root == def)
                    || stack
                        .iter()
                        .any(|(active, active_args)| active == def && active_args == args)
                {
                    return true;
                }
                let bindings: BTreeMap<LocalId, TirGenericArg> = generics
                    .iter()
                    .zip(args)
                    .filter_map(|(generic, arg)| Some((generic.id?, arg.clone())))
                    .collect();
                stack.push((*def, args.clone()));
                let recursive = fields.iter().any(|field| {
                    self.type_from_mir_type_ref(*def, &field.ty)
                        .ok()
                        .is_some_and(|ty| {
                            self.recursive_value_type(&ty.substitute(&bindings), stack)
                        })
                });
                stack.pop();
                recursive
            }
            _ => false,
        }
    }

    fn value_type_fields(
        &self,
        def: DefId,
    ) -> Option<(&[HirFieldDecl], &[HirSignatureGenericParam])> {
        self.hir
            .structs()
            .get(&def)
            .map(|item| (item.fields.as_slice(), item.generics.as_slice()))
            .or_else(|| {
                self.hir
                    .bundles()
                    .get(&def)
                    .map(|item| (item.fields.as_slice(), item.generics.as_slice()))
            })
    }
}
