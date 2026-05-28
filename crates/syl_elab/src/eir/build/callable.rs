use crate::{
    CompileError,
    const_eval::ConstValueElaborator,
    eir::{EirModule, EirParam, EirRawDesign},
    map_ir::MapIrProgram,
    program::{
        ElabCallable, ElabCallableItem, ElabExternCellItem, ElabProgram, ElabSignatureGenericParam,
    },
};
use syl_hir::DefId;

use super::Env;

#[non_exhaustive]
pub(crate) struct EirBuilder<'a, C>
where
    C: ConstValueElaborator + ?Sized,
{
    pub(crate) const_elaborator: &'a C,
    pub(crate) map_ir: &'a MapIrProgram,
    pub(crate) program: &'a ElabProgram,
}

#[non_exhaustive]
pub(crate) struct Elaborator<'a, C>
where
    C: ConstValueElaborator + ?Sized,
{
    program: &'a ElabProgram,
    const_elaborator: &'a C,
    map_ir: &'a MapIrProgram,
}

impl<'a, C> Elaborator<'a, C>
where
    C: ConstValueElaborator + ?Sized,
{
    pub(crate) fn new(
        program: &'a ElabProgram,
        const_elaborator: &'a C,
        map_ir: &'a MapIrProgram,
    ) -> Self {
        Self {
            program,
            const_elaborator,
            map_ir,
        }
    }

    pub(crate) fn build_raw_design(self) -> Result<EirRawDesign, CompileError> {
        let _map_ir_nodes = self.map_ir.len();
        EirBuilder::new(self.program, self.const_elaborator, self.map_ir).build_raw_design()
    }
}

impl<'a, C> EirBuilder<'a, C>
where
    C: ConstValueElaborator + ?Sized,
{
    pub(crate) fn new(
        program: &'a ElabProgram,
        const_elaborator: &'a C,
        map_ir: &'a MapIrProgram,
    ) -> Self {
        Self {
            const_elaborator,
            map_ir,
            program,
        }
    }

    pub(crate) fn build_raw_design(&self) -> Result<EirRawDesign, CompileError> {
        let mut modules = Vec::new();
        for (owner, callable) in self.program.callables() {
            match callable {
                ElabCallable::Cell(item) => {
                    modules.push(self.build_callable(*owner, item)?);
                }
                ElabCallable::Extern(item) => {
                    modules.push(self.build_extern(*owner, item)?);
                }
            }
        }
        Ok(EirRawDesign::new(modules))
    }

    fn build_callable(
        &self,
        owner: DefId,
        item: &ElabCallableItem,
    ) -> Result<EirModule, CompileError> {
        let mut env = Env::with_owner(owner);
        for generic in &item.generics {
            self.insert_generic(&mut env, generic);
        }
        let mut ports = Vec::new();
        for param in &item.params {
            let param_ty = param.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                super::connections::PortSpec {
                    doc: param.doc.as_deref(),
                    name: &param.name,
                    dir: param.direction,
                    ty: &param_ty,
                    span: param.span,
                },
            )?;
        }
        if let Some(result) = &item.result {
            let result_ty = result.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                super::connections::PortSpec {
                    doc: result.doc.as_deref(),
                    name: &result.name,
                    dir: crate::program::ElabPortDirection::Out,
                    ty: &result_ty,
                    span: result.span,
                },
            )?;
        }
        let params = self.generic_params(&env, &item.generics);
        let items = self.emit_body(&item.body, &mut env)?;
        Ok(EirModule::new(&item.name, params, ports, items).with_doc(item.doc.clone()))
    }

    fn build_extern(
        &self,
        owner: DefId,
        item: &ElabExternCellItem,
    ) -> Result<EirModule, CompileError> {
        let mut env = Env::with_owner(owner);
        for generic in &item.generics {
            self.insert_generic(&mut env, generic);
        }
        let mut ports = Vec::new();
        for param in &item.params {
            let param_ty = param.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                super::connections::PortSpec {
                    doc: param.doc.as_deref(),
                    name: &param.name,
                    dir: param.direction,
                    ty: &param_ty,
                    span: param.span,
                },
            )?;
        }
        if let Some(result) = &item.result {
            let result_ty = result.ty.clone();
            self.add_port(
                &mut ports,
                &mut env,
                super::connections::PortSpec {
                    doc: result.doc.as_deref(),
                    name: &result.name,
                    dir: crate::program::ElabPortDirection::Out,
                    ty: &result_ty,
                    span: result.span,
                },
            )?;
        }
        let params = self.generic_params(&env, &item.generics);
        Ok(EirModule::new_extern(&item.name, params, ports).with_doc(item.doc.clone()))
    }

    fn generic_params(&self, env: &Env, params: &[ElabSignatureGenericParam]) -> Vec<EirParam> {
        let mut out = Vec::new();
        for param in params {
            if self.is_domain_param(param) {
                continue;
            }
            if param.kind.is_none() {
                out.push(
                    EirParam::new(format!("{}_WIDTH", param.name), "1").with_doc(param.doc.clone()),
                );
                continue;
            }
            let default = param
                .default
                .as_ref()
                .map(|expr| self.elab_expr(expr, env))
                .map(|expr| expr.fact_key())
                .unwrap_or_else(|| "1".to_string());
            out.push(EirParam::new(&param.name, default).with_doc(param.doc.clone()));
        }
        out
    }

    pub(crate) fn is_domain_param(&self, param: &ElabSignatureGenericParam) -> bool {
        param
            .kind
            .as_ref()
            .is_some_and(|kind| kind.path_name().is_some_and(|name| name == "Domain"))
    }

    pub(crate) fn insert_generic(&self, env: &mut Env, param: &ElabSignatureGenericParam) {
        if let Some(kind) = &param.kind
            && matches!(self.static_type_name(kind), Some("Nat" | "Bool"))
        {
            env.insert(
                &param.name,
                crate::eir::EirExpr::ident(&param.name),
                kind.clone(),
            );
        }
    }

    pub(crate) fn sanitize(&self, name: &str) -> String {
        name.chars()
            .map(|ch| {
                if ch.is_ascii_alphanumeric() || ch == '_' {
                    ch
                } else {
                    '_'
                }
            })
            .collect()
    }
}
