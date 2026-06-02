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
        self.check_hardware_block(&item.body, &env, HardwareBlockMode::Normal, errors)
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
mod tests {
    use crate::HirResolver;
    use crate::{
        MiddleCompiler,
        tir::{BindingRef, TypePhaseChecker},
    };
    use syl_span::Span;
    use syl_syntax::{AstFile, Block, CallableItem, Expr, Item, MapItem, Stmt, TypeExpr};

    #[test]
    fn check_tir_partial_keeps_valid_item_hover_after_other_item_error() {
        let valid_expr_span = Span::new(110, 111);
        let error_span = Span::new(210, 211);
        let files = vec![AstFile::new(vec![
            valid_map("Good", valid_expr_span),
            bad_module("Bad", error_span),
        ])];
        let hir = MiddleCompiler::new()
            .session(&files)
            .resolve_hir()
            .expect("HIR should resolve before TIR recovery");
        let output = hir.check_tir_partial();

        assert!(!output.diagnostics().is_empty());
        let stage = output
            .partial_stage()
            .expect("recoverable TIR errors should keep collected typed facts");
        let hover = stage
            .hover_at(valid_expr_span)
            .expect("valid item expression should still have typed hover");

        assert_eq!(hover.text(), "Comb nat");
    }

    #[test]
    fn check_tir_partial_returns_stage_with_diagnostics_for_recoverable_error() {
        let error_span = Span::new(30, 31);
        let files = vec![AstFile::new(vec![
            valid_typed_map("Good", Span::new(70, 71)),
            bad_module("Bad", error_span),
        ])];
        let hir = MiddleCompiler::new()
            .session(&files)
            .resolve_hir()
            .expect("HIR should resolve before TIR recovery");
        let output = hir.check_tir_partial();

        assert!(!output.diagnostics().is_empty());
        assert!(
            output.stage().is_some(),
            "recoverable TIR diagnostics must not imply a missing stage"
        );
    }

    #[test]
    fn check_tir_partial_keeps_valid_fact_after_same_item_error() {
        let error_span = Span::new(30, 31);
        let valid_span = Span::new(60, 61);
        let files = vec![AstFile::new(vec![Item::Module(
            CallableItem::builder(
                "Mixed".to_string(),
                Block::new(
                    vec![
                        Stmt::ElabIf {
                            cond: Expr::Int(1, error_span),
                            then_block: Block::new(Vec::new(), None, Span::new(32, 34)),
                            else_block: None,
                            span: Span::new(20, 34),
                        },
                        Stmt::Let {
                            name: "ok".to_string(),
                            ty: None,
                            value: Some(Expr::Int(7, valid_span)),
                            span: Span::new(50, 70),
                        },
                    ],
                    None,
                    Span::new(10, 80),
                ),
            )
            .span(Span::new(0, 80))
            .build(),
        )])];
        let hir = MiddleCompiler::new()
            .session(&files)
            .resolve_hir()
            .expect("HIR should resolve before TIR recovery");
        let output = hir.check_tir_partial();

        assert!(!output.diagnostics().is_empty());
        let stage = output
            .partial_stage()
            .expect("same-item recovery should keep partial TIR facts");
        let hover = stage
            .hover_at(valid_span)
            .expect("later valid let expression should still have typed hover");

        assert_eq!(hover.text(), "Hardware nat");
    }

    #[test]
    fn check_output_keeps_recorded_binding_types_with_diagnostics() {
        let error_span = Span::new(80, 81);
        let files = vec![AstFile::new(vec![
            valid_typed_map("Good", Span::new(40, 41)),
            bad_module("Bad", error_span),
        ])];
        let hir = HirResolver::new(&files)
            .resolve()
            .expect("HIR should resolve before TIR recovery");
        let good = hir
            .defs
            .iter()
            .find(|def| def.name == "Good")
            .expect("valid map def should exist");
        let param = hir
            .locals
            .iter()
            .find(|local| local.owner == good.id && local.name == "x")
            .expect("valid map param should exist");
        let param_id = param.id;
        let output = TypePhaseChecker::new(std::sync::Arc::new(hir)).check_output();

        assert!(!output.diagnostics().is_empty());
        let tir = output
            .stage()
            .expect("TIR output should keep partial facts after recoverable errors");
        assert_eq!(
            tir.binding_type_label(BindingRef::Local(param_id))
                .as_deref(),
            Some("nat")
        );
    }

    fn valid_map(name: &str, expr_span: Span) -> Item {
        Item::Map(
            MapItem::builder(name.to_string(), Expr::Int(1, expr_span))
                .span(Span::new(100, 120))
                .build(),
        )
    }

    fn valid_typed_map(name: &str, expr_span: Span) -> Item {
        Item::Map(
            MapItem::builder(name.to_string(), Expr::Ident("x".to_string(), expr_span))
                .params(vec![syl_syntax::Param::new(
                    "x".to_string(),
                    None,
                    TypeExpr::Path(vec!["nat".to_string()], Span::new(12, 15)),
                    Span::new(10, 15),
                )])
                .span(Span::new(0, 50))
                .build(),
        )
    }

    fn bad_module(name: &str, cond_span: Span) -> Item {
        Item::Module(
            CallableItem::builder(
                name.to_string(),
                Block::new(
                    vec![Stmt::ElabIf {
                        cond: Expr::Int(1, cond_span),
                        then_block: Block::new(Vec::new(), None, Span::new(220, 222)),
                        else_block: None,
                        span: Span::new(200, 222),
                    }],
                    None,
                    Span::new(190, 230),
                ),
            )
            .span(Span::new(180, 240))
            .build(),
        )
    }
}
