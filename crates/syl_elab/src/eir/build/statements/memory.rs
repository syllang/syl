use super::{EirBuilder, Env};
use crate::{
    CompileError, EirError,
    eir::{EirBound, EirExpr, EirItem, EirPlace},
    mir::MirTypeRef,
    program::{ElabExpr, ElabExprNode},
};
use syl_span::Span;

impl<C: crate::const_eval::ConstValueElaborator + ?Sized> EirBuilder<'_, C> {
    pub(super) fn emit_mem(
        &self,
        name: &str,
        ty: &MirTypeRef,
        span: Span,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let ty = self.subst_type_vars(ty, &env.type_replacements);
        let (len, elem) = ty
            .array()
            .ok_or_else(|| CompileError::lowering_at(EirError::MemoryRequiresArrayType, span))?;
        let name_physical = env.local_name(name);
        let item = EirItem::MemoryDecl {
            name: name_physical.clone(),
            elem_width: self.width_bound(env.owner, elem),
            depth: EirBound::new(self.array_len_key(len), self.array_len_expr(len)),
            origin: env.origin(span),
        };
        env.insert(name, EirExpr::ident(name_physical), ty);
        env.vars
            .get_mut(name)
            .expect("memory just inserted")
            .is_memory = true;
        Ok(vec![item])
    }

    pub(super) fn try_emit_memory_stmt(
        &self,
        expr: &ElabExpr,
        env: &Env,
    ) -> Result<Option<Vec<EirItem>>, CompileError> {
        let (args, read) = if let Some(args) = self.builtin_stmt_args(expr, env, "mem_write") {
            (args, false)
        } else if let Some(args) = self.builtin_stmt_args(expr, env, "mem_read_sync") {
            (args, true)
        } else {
            return Ok(None);
        };
        if args.len() != 5 || args.iter().any(|a| a.name.is_some()) {
            return Err(CompileError::lowering_at(
                EirError::MemoryOpStatementOnly,
                expr.span(),
            ));
        }
        let ElabExprNode::Ident(name) = &args[0].value.node else {
            return Err(CompileError::lowering_at(
                EirError::MemoryTargetMustBeMem {
                    name: "<expression>".into(),
                },
                expr.span(),
            ));
        };
        let var = env.var(name).filter(|v| v.is_memory).ok_or_else(|| {
            CompileError::lowering_at(
                EirError::MemoryTargetMustBeMem { name: name.clone() },
                expr.span(),
            )
        })?;
        let memory = var.code.fact_key();
        let clock = self.elab_expr(&args[1].value, env);
        let enable = self.elab_expr(&args[2].value, env);
        let addr = self.elab_expr(&args[3].value, env);
        let mut reads = Vec::new();
        for arg in &args[2..if read { 4 } else { 5 }] {
            reads.extend(self.elab_read_places(&arg.value, env));
        }
        reads.sort_by_key(EirExpr::fact_key);
        reads.dedup_by_key(|r| r.fact_key());
        let origin = env.origin(expr.span());
        if !read {
            return Ok(Some(vec![EirItem::MemoryWrite {
                clock,
                memory,
                enable,
                addr,
                data: self.elab_expr(&args[4].value, env),
                reads,
                origin,
            }]));
        }
        let (_, elem) = var.ty.array().expect("memory declaration requires array");
        let rd_reg = env.unique_label(&format!("{memory}_rd"), expr.span());
        let rd_value = EirExpr::ident(&rd_reg);
        Ok(Some(vec![
            EirItem::Storage {
                width: self.width_bound(env.owner, elem),
                name: rd_reg.clone(),
                origin: origin.clone(),
            },
            EirItem::MemorySyncRead {
                clock,
                memory,
                enable,
                addr,
                target: EirPlace::Ident(rd_reg),
                reads,
                origin: origin.clone(),
            },
            EirItem::Drive {
                lhs: self.place_expr(&args[4].value, env)?,
                rhs: rd_value.clone(),
                reads: vec![rd_value],
                origin,
            },
        ]))
    }
}
