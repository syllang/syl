use super::super::values::SymbolicConstValue;
use super::{EirBuilder, Env};
use crate::{
    CompileError, EirError, eir::EirExpr, mir::MirTypeRef, program::ElabExpr, tir::TirType,
};
use syl_hir::FormatSpec;
use syl_hw::{FormatPiece, FormatStyle};

impl<C: crate::const_eval::ConstValueElaborator + ?Sized> EirBuilder<'_, C> {
    pub(super) fn format_value(
        &self,
        expr: &ElabExpr,
        spec: &FormatSpec,
        env: &Env,
        out: &mut Vec<FormatPiece<EirExpr>>,
    ) -> Result<(), CompileError> {
        let ty = env.owner.and_then(|o| self.program.expr_type(o, expr));
        if matches!(
            ty,
            Some(TirType::Named {
                kind: Some(syl_hir::HirDefKind::Struct),
                ..
            })
        ) {
            let value = self.elab_const_value(expr, env)?;
            let symbolic = self.symbolic_elab_const_value(expr, env)?;
            return format_const(&value, symbolic.as_ref(), spec, expr.span(), out);
        }
        let ty = ty
            .and_then(|t| self.tir_to_mir_type(t, expr.span()))
            .ok_or_else(|| {
                CompileError::lowering_at(
                    EirError::InvalidFormat {
                        message: "cannot determine formatted value type".into(),
                    },
                    expr.span(),
                )
            })?;
        let ty = self.subst_type_vars(&ty, &env.type_replacements);
        self.format_declared((self.elab_expr(expr, env), &ty), spec, env, out)
    }

    fn format_declared(
        &self,
        field_value: (EirExpr, &MirTypeRef),
        spec: &FormatSpec,
        env: &Env,
        out: &mut Vec<FormatPiece<EirExpr>>,
    ) -> Result<(), CompileError> {
        let (value, ty) = field_value;
        if ty.array().is_some() || ty.view_select().is_some() {
            return Err(CompileError::lowering_at(
                EirError::InvalidFormat {
                    message: "formatted aggregate fields must be scalar values or bundles".into(),
                },
                ty.span(),
            ));
        }
        if let Some(bundle) = self.bundle_for_type(env.owner, ty) {
            out.push(FormatPiece::Literal("{ ".into()));
            for (i, field) in bundle.fields.iter().enumerate() {
                out.push(FormatPiece::Literal(format!(
                    "{}{}: ",
                    if i == 0 { "" } else { ", " },
                    field.name
                )));
                let field_ty = self.subst_bundle_field_type(env.owner, ty, &field.ty);
                let field_value = self
                    .bundle_field_ref(env.owner, &value, ty, &field.name)
                    .expect("field from this bundle");
                self.format_declared((field_value, &field_ty), spec, env, out)?;
            }
            out.push(FormatPiece::Literal(" }".into()));
            return Ok(());
        }
        let name = ty.type_name().unwrap_or("");
        let style = if matches!(spec, FormatSpec::Default | FormatSpec::Debug)
            && let Some(id) = self.type_def_id(env.owner, ty)
            && self.program.enum_by_def(id).is_some()
        {
            let variants = self
                .program
                .enum_variants
                .iter()
                .filter(|(k, _)| k.enum_def == id)
                .map(|(k, v)| (*v, k.name.clone()))
                .collect();
            FormatStyle::Enum {
                name: name.into(),
                variants,
                debug: *spec == FormatSpec::Debug,
            }
        } else {
            scalar_style(spec, name)
        };
        out.push(FormatPiece::Value {
            expr: value,
            width: (name != "nat").then(|| self.width(env.owner, ty)),
            style,
        });
        Ok(())
    }
}

fn format_const(
    value: &crate::const_eval::ConstValue,
    symbolic: Option<&SymbolicConstValue>,
    spec: &FormatSpec,
    span: syl_span::Span,
    out: &mut Vec<FormatPiece<EirExpr>>,
) -> Result<(), CompileError> {
    use crate::const_eval::ConstValue;
    match value {
        ConstValue::Struct(s) => {
            out.push(FormatPiece::Literal("{ ".into()));
            for (i, field) in s.fields().iter().enumerate() {
                out.push(FormatPiece::Literal(format!(
                    "{}{}: ",
                    if i == 0 { "" } else { ", " },
                    field.name()
                )));
                let child = match symbolic {
                    Some(SymbolicConstValue::Struct(fields)) => fields.get(field.name()),
                    _ => None,
                };
                format_const(field.value(), child, spec, span, out)?;
            }
            out.push(FormatPiece::Literal(" }".into()));
        }
        ConstValue::Nat(n) => out.push(FormatPiece::Value {
            expr: EirExpr::Int(*n),
            width: None,
            style: scalar_style(spec, "nat"),
        }),
        ConstValue::Bool(b) => out.push(FormatPiece::Value {
            expr: EirExpr::Bool(*b),
            width: Some("1".into()),
            style: scalar_style(spec, "bool"),
        }),
        ConstValue::Unknown(kind)
            if let Some(SymbolicConstValue::Scalar(expr)) = symbolic
                && matches!(
                    kind,
                    crate::const_eval::ConstKind::Nat | crate::const_eval::ConstKind::Bool
                ) =>
        {
            let boolean = matches!(kind, crate::const_eval::ConstKind::Bool);
            out.push(FormatPiece::Value {
                expr: expr.clone(),
                width: boolean.then(|| "1".into()),
                style: scalar_style(spec, if boolean { "bool" } else { "nat" }),
            });
        }
        _ => {
            return Err(CompileError::lowering_at(
                EirError::InvalidFormat {
                    message: "struct field has no elaborated value".into(),
                },
                span,
            ));
        }
    }
    Ok(())
}

fn scalar_style(spec: &FormatSpec, ty: &str) -> FormatStyle {
    match *spec {
        FormatSpec::Default if ty == "SInt" => FormatStyle::Signed,
        FormatSpec::Default if ty == "bool" => FormatStyle::Bool,
        FormatSpec::Default | FormatSpec::Unsigned => FormatStyle::Unsigned,
        FormatSpec::Signed => FormatStyle::Signed,
        FormatSpec::Debug => FormatStyle::Debug,
        FormatSpec::Hex {
            upper,
            prefix,
            digits,
        } => FormatStyle::Hex {
            upper,
            prefix,
            digits,
        },
        FormatSpec::Binary { prefix, digits } => FormatStyle::Binary { prefix, digits },
    }
}
