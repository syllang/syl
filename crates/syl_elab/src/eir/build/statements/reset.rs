use super::{EirBuilder, Env};
use crate::{
    CompileError, EirError,
    eir::{EirExpr, EirPortDomain, EirReset, EirResetMode, EirResetPolarity},
    mir::MirTypeRef,
    program::{ElabExpr, ElabExprNode, ElabRegReset},
};
use syl_hir::{HirResetMode, HirResetPolarity};
use syl_span::Span;

impl<C: crate::const_eval::ConstValueElaborator + ?Sized> EirBuilder<'_, C> {
    pub(in crate::eir::build) fn reset_domain<'b>(
        &self,
        ty: &'b MirTypeRef,
    ) -> Option<&'b MirTypeRef> {
        self.first_type_arg(ty).filter(|a| {
            !matches!(
                a.path_name(),
                Some("Sync" | "Async" | "ActiveHigh" | "ActiveLow")
            )
        })
    }

    pub(in crate::eir::build) fn port_domain(&self, ty: &MirTypeRef) -> EirPortDomain {
        match self.static_type_name(ty) {
            Some("Clock") => EirPortDomain::Clock {
                domain: self
                    .first_type_arg(ty)
                    .and_then(MirTypeRef::path_name)
                    .map(str::to_owned),
            },
            Some("Reset") => EirPortDomain::Reset {
                domain: self
                    .reset_domain(ty)
                    .and_then(MirTypeRef::path_name)
                    .map(str::to_owned),
            },
            _ => EirPortDomain::Data,
        }
    }

    fn reset_var<'b>(&self, expr: &ElabExpr, env: &'b Env) -> Option<&'b super::super::VarInfo> {
        match &expr.node {
            ElabExprNode::Ident(name) => env.var(name),
            ElabExprNode::Group(inner) => self.reset_var(inner, env),
            _ => {
                let code = self.elab_expr(expr, env);
                env.vars
                    .values()
                    .find(|v| v.code == code && self.static_type_name(&v.ty) == Some("Reset"))
            }
        }
    }

    pub(super) fn register_clock_reset(
        &self,
        reset: Option<&ElabRegReset>,
        span: Span,
        env: &Env,
        reads: &mut Vec<EirExpr>,
    ) -> Result<(EirExpr, Option<EirReset>), CompileError> {
        let var = reset
            .and_then(|r| r.domain.as_ref())
            .and_then(|r| self.reset_var(r, env));
        let matching = reset
            .and_then(|r| r.domain.as_ref())
            .and_then(|r| env.clock_for_elab_reset_expr(r, self));
        let fallback = env.single_by_type("Clock", self);
        if matching.is_none()
            && let Some(var) = var
            && let Some(domain) = self.reset_domain(&var.ty)
            && let Some(clock) = fallback.as_ref()
        {
            let clock_var = env
                .vars
                .values()
                .find(|v| v.code == *clock && self.static_type_name(&v.ty) == Some("Clock"));
            let clock_domain = clock_var
                .and_then(|v| self.first_type_arg(&v.ty))
                .and_then(MirTypeRef::path_name)
                .unwrap_or("<unspecified>");
            return Err(CompileError::lowering_at(
                EirError::ResetClockDomainMismatch {
                    reset: var.code.fact_key(),
                    reset_domain: domain.path_name().unwrap_or("<unknown>").into(),
                    clock: clock.fact_key(),
                    clock_domain: clock_domain.into(),
                },
                span,
            ));
        }
        let clock = matching
            .or(fallback)
            .ok_or_else(|| CompileError::lowering_at(EirError::RegisterRequiresClock, span))?;
        let Some(reset) = reset else {
            return Ok((clock, None));
        };
        let condition = reset
            .domain
            .as_ref()
            .map(|e| self.elab_expr(e, env))
            .or_else(|| env.single_by_type("Reset", self))
            .ok_or_else(|| {
                CompileError::lowering_at(EirError::RegisterRequiresReset, reset.span)
            })?;
        let (mode, polarity) = self.reset_config_for_code(&condition, env);
        let conflict = reset
            .mode
            .is_some_and(|m| matches!(m, HirResetMode::Async) != (mode == EirResetMode::Async))
            || reset.polarity.is_some_and(|p| {
                matches!(p, HirResetPolarity::ActiveLow)
                    != (polarity == EirResetPolarity::ActiveLow)
            });
        if conflict {
            return Err(CompileError::lowering_at(
                EirError::ResetConfigConflict {
                    name: condition.fact_key(),
                    detail: "register qualifiers disagree with the Reset type".into(),
                },
                reset.span,
            ));
        }
        reads.extend(self.elab_read_places(&reset.value, env));
        Ok((
            clock,
            Some(
                EirReset::new(condition, self.elab_expr(&reset.value, env))
                    .with_mode(mode)
                    .with_polarity(polarity),
            ),
        ))
    }

    pub(super) fn reset_config_for_code(
        &self,
        condition: &EirExpr,
        env: &Env,
    ) -> (EirResetMode, EirResetPolarity) {
        let ty = env
            .vars
            .values()
            .find(|v| v.code == *condition && self.static_type_name(&v.ty) == Some("Reset"))
            .map(|v| &v.ty);
        let args = ty.and_then(MirTypeRef::args).unwrap_or_default();
        (
            if args.iter().any(|a| a.path_name() == Some("Async")) {
                EirResetMode::Async
            } else {
                EirResetMode::Sync
            },
            if args.iter().any(|a| a.path_name() == Some("ActiveLow")) {
                EirResetPolarity::ActiveLow
            } else {
                EirResetPolarity::ActiveHigh
            },
        )
    }
}
