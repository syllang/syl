mod body;
mod expr;

use super::consts::TirConstEnv;
use super::enum_layout;
use super::{BindingKind, BindingRef, HardwareBlockMode, Phase, TypePhaseChecker};
use crate::{
    CompileError,
    hir::{
        HirBundleItem, HirCallable, HirCallableItem, HirConstItem, HirEnumItem, HirExternCellItem,
        HirFnItem, HirInterfaceItem, HirMapItem, HirSignatureGenericParam, HirSignatureParam,
        HirSignatureResultBinding,
    },
};
use syl_hir::DefId;

impl TypePhaseChecker {
    pub(super) fn check_const(
        &mut self,
        owner: DefId,
        item: &HirConstItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.record_def_binding(owner, BindingKind::Const);
        if let Some(ty) = &item.ty
            && let Some(ty) =
                Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, ty))
        {
            self.record_binding_type(BindingRef::Def(owner), ty);
        }
        Self::record_recoverable(errors, self.record_phase(&item.value, Phase::Const));
        Ok(())
    }

    pub(super) fn check_fn(
        &mut self,
        owner: DefId,
        item: &HirFnItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.record_def_binding(owner, BindingKind::Const);
        self.check_params(owner, &item.params, errors)?;
        if let Some(ret_ty) = &item.ret_ty
            && let Some(ty) =
                Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &ret_ty.ty))
        {
            self.record_binding_type(BindingRef::Def(owner), ty);
        }
        Ok(())
    }

    pub(super) fn check_bundle(
        &mut self,
        owner: DefId,
        item: &HirBundleItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.check_generics(owner, &item.generics, errors)?;
        for field in &item.fields {
            Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &field.ty));
        }
        Ok(())
    }

    pub(super) fn check_enum(
        &mut self,
        owner: DefId,
        item: &HirEnumItem,
    ) -> Result<(), CompileError> {
        let values = enum_layout::resolve_enum_values(self, owner, item)?;
        self.enum_variant_values.extend(values);
        Ok(())
    }

    pub(super) fn check_interface(
        &mut self,
        owner: DefId,
        item: &HirInterfaceItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.check_generics(owner, &item.generics, errors)?;
        for field in &item.fields {
            Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &field.ty));
        }
        Ok(())
    }

    pub(super) fn check_callable(
        &mut self,
        owner: DefId,
        callable: &HirCallable,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        match callable {
            HirCallable::Cell(item) => self.check_callable_item(item, errors),
            HirCallable::Extern(item) => self.check_extern_module(owner, item, errors),
            _ => Ok(()),
        }
    }

    pub(super) fn check_callable_item(
        &mut self,
        item: &HirCallableItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let owner = self.current_owner()?;
        self.check_generics(owner, &item.generics, errors)?;
        self.check_params(owner, &item.params, errors)?;
        if let Some(result) = &item.result {
            self.check_result(owner, result, errors)?;
        }
        let env = TirConstEnv::from_generics(owner, &item.generics, self);
        self.check_hardware_block(&item.body, &env, HardwareBlockMode::Normal, errors)?;
        Ok(())
    }

    pub(super) fn check_extern_module(
        &mut self,
        owner: DefId,
        item: &HirExternCellItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.check_generics(owner, &item.generics, errors)?;
        self.check_params(owner, &item.params, errors)?;
        if let Some(result) = &item.result {
            self.check_result(owner, result, errors)?;
        }
        Ok(())
    }

    pub(super) fn check_generics(
        &mut self,
        owner: DefId,
        generics: &[HirSignatureGenericParam],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        for generic in generics {
            let Some(id) = Self::record_recoverable(
                errors,
                self.record_decl_local_binding(
                    &generic.name,
                    generic.id,
                    generic.span,
                    BindingKind::Generic,
                ),
            ) else {
                continue;
            };
            if let Some(kind) = &generic.kind
                && let Some(ty) =
                    Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, kind))
            {
                self.record_binding_type(BindingRef::Local(id), ty);
            }
        }
        Ok(())
    }

    pub(super) fn check_params(
        &mut self,
        owner: DefId,
        params: &[HirSignatureParam],
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        for param in params {
            let Some(id) = Self::record_recoverable(
                errors,
                self.record_decl_local_binding(
                    &param.name,
                    param.id,
                    param.span,
                    BindingKind::Port,
                ),
            ) else {
                continue;
            };
            if let Some(ty) =
                Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &param.ty))
            {
                self.record_binding_type(BindingRef::Local(id), ty);
            }
        }
        Ok(())
    }

    pub(super) fn check_result(
        &mut self,
        owner: DefId,
        result: &HirSignatureResultBinding,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        let Some(id) = Self::record_recoverable(
            errors,
            self.record_decl_local_binding(
                &result.name,
                result.id,
                result.span,
                BindingKind::Local,
            ),
        ) else {
            return Ok(());
        };
        if let Some(ty) =
            Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &result.ty))
        {
            self.record_binding_type(BindingRef::Local(id), ty);
        }
        Ok(())
    }

    pub(super) fn check_map(
        &mut self,
        owner: DefId,
        map: &HirMapItem,
        errors: &mut Vec<CompileError>,
    ) -> Result<(), CompileError> {
        self.record_def_binding(owner, BindingKind::Const);
        self.check_generics(owner, &map.generics, errors)?;
        self.check_params(owner, &map.params, errors)?;
        if let Some(ret_ty) = &map.ret_ty
            && let Some(ty) =
                Self::record_recoverable(errors, self.type_from_mir_type_ref(owner, &ret_ty.ty))
        {
            self.record_binding_type(BindingRef::Def(owner), ty);
        }
        self.check_map_expr(&map.body, errors)
    }
}

#[cfg(any())]
mod tests;
