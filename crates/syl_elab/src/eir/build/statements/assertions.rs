use crate::{
    CompileError, DriverError, EirError,
    eir::{EirBinaryOp, EirExpr, EirItem, EirResetPolarity, EirUnaryOp},
    mir::MirTypeRef,
    program::{ElabCallArg, ElabExpr},
};
use syl_hir::FormatPart;
use syl_hw::FormatPiece;

use super::{EirBuilder, Env};

impl<C: crate::const_eval::ConstValueElaborator + ?Sized> EirBuilder<'_, C> {
    pub(super) fn try_emit_assert_stmt(
        &self,
        expr: &ElabExpr,
        env: &Env,
        allow: bool,
    ) -> Result<Option<Vec<EirItem>>, CompileError> {
        let (log, args) = if let Some(args) = self.builtin_stmt_args(expr, env, "log") {
            (true, args)
        } else if let Some(args) = self.builtin_stmt_args(expr, env, "assert") {
            (false, args)
        } else {
            return Ok(None);
        };
        if !allow {
            return Err(CompileError::lowering_at(
                if log {
                    EirError::LogStatementOnly
                } else {
                    EirError::AssertionStatementOnly
                },
                expr.span(),
            ));
        }
        let named = |name| {
            args.iter()
                .find(|arg| arg.name.as_deref() == Some(name))
                .map(|arg| &arg.value)
        };
        let clock = named("clk")
            .map(|clk| self.elab_expr(clk, env))
            .or_else(|| env.single_by_type("Clock", self))
            .ok_or_else(|| {
                CompileError::lowering_at(
                    if log {
                        EirError::InvalidFormat {
                            message: "log requires a unique Clock or an explicit clk: argument"
                                .into(),
                        }
                    } else {
                        EirError::AssertionRequiresClock
                    },
                    expr.span(),
                )
            })?;
        let mut positional = args.iter().filter(|arg| arg.name.is_none());
        let mut reads = vec![clock.clone()];
        let mut trigger = if log {
            named("when").map_or(EirExpr::Bool(true), |v| {
                reads.extend(self.elab_read_places(v, env));
                self.elab_expr(v, env)
            })
        } else {
            let cond = &positional
                .next()
                .ok_or_else(|| {
                    CompileError::lowering_at(
                        EirError::AssertionRequiresSingleCondition,
                        expr.span(),
                    )
                })?
                .value;
            reads.extend(self.elab_read_places(cond, env));
            EirExpr::unary(EirUnaryOp::Not, self.elab_expr(cond, env))
        };
        let reset = named("rst")
            .map(|r| self.elab_expr(r, env))
            .or_else(|| env.reset_for_clock(&clock, self));
        if let Some(reset) = reset {
            let typed = |code: &EirExpr, kind| {
                env.vars
                    .values()
                    .find(|v| v.code == *code && v.ty.type_name() == Some(kind))
                    .map(|v| &v.ty)
            };
            let reset_domain = typed(&reset, "Reset")
                .and_then(|ty| self.reset_domain(ty))
                .and_then(MirTypeRef::path_name);
            let clock_domain = typed(&clock, "Clock")
                .and_then(|ty| self.first_type_arg(ty))
                .and_then(MirTypeRef::path_name);
            if let Some(domain) = reset_domain
                && Some(domain) != clock_domain
            {
                return Err(CompileError::driver_error(
                    DriverError::CdcViolation {
                        source_domain: domain.into(),
                        target_domain: clock_domain.unwrap_or("Clock").into(),
                        signal: reset.fact_key(),
                        target: if log { "log" } else { "assert" }.into(),
                    },
                    expr.span(),
                ));
            }
            reads.push(reset.clone());
            let active_low =
                self.reset_config_for_code(&reset, env).1 == EirResetPolarity::ActiveLow;
            trigger = EirExpr::binary(
                EirBinaryOp::BitAnd,
                if active_low {
                    reset
                } else {
                    EirExpr::unary(EirUnaryOp::Not, reset)
                },
                trigger,
            );
        }
        let message = if let Some(format) = positional.next() {
            let values: Vec<_> = positional.collect();
            self.format_message(format, &values, env, &mut reads)?
        } else {
            vec![FormatPiece::Literal("assert failed".into())]
        };
        reads.sort_by_key(EirExpr::fact_key);
        reads.dedup_by_key(|r| r.fact_key());
        let origin = env.origin(expr.span());
        Ok(Some(vec![if log {
            EirItem::ClockedLog {
                clock,
                trigger,
                reads,
                message,
                origin,
            }
        } else {
            EirItem::ClockedAssert {
                clock,
                trigger,
                reads,
                message,
                origin,
            }
        }]))
    }

    fn format_message(
        &self,
        msg: &ElabCallArg,
        args: &[&ElabCallArg],
        env: &Env,
        reads: &mut Vec<EirExpr>,
    ) -> Result<Vec<FormatPiece<EirExpr>>, CompileError> {
        let mut out = Vec::new();
        let malformed = || {
            CompileError::lowering_at(
                EirError::InvalidFormat {
                    message: "invalid format arguments".into(),
                },
                msg.span,
            )
        };
        for part in msg.format.as_ref().ok_or_else(malformed)? {
            let (expr, spec) = match part {
                FormatPart::Literal(s) => {
                    out.push(FormatPiece::Literal(s.clone()));
                    continue;
                }
                FormatPart::Capture { expr, spec } => (expr, spec),
                FormatPart::Argument { index, spec } => {
                    (&args.get(*index).ok_or_else(malformed)?.value, spec)
                }
            };
            reads.extend(self.elab_read_places(expr, env));
            self.format_value(expr, spec, env, &mut out)?;
        }
        Ok(out)
    }
}
