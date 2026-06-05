use crate::{
    CompileError, EirError,
    const_eval::ConstValue,
    eir::EirExpr,
    eir::EirItem,
    mir::MirTypeRef,
    program::{ElabBlock, ElabExprNode},
};

use super::super::connections::InstanceEmitRequest;
use super::{EirBuilder, Env, ForEmit};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    pub(super) fn emit_for(
        &self,
        request: ForEmit<'_>,
        env: &mut Env,
    ) -> Result<Vec<EirItem>, CompileError> {
        let ElabExprNode::Range { start, end } = &request.range_expr.node else {
            return Err(CompileError::lowering_at(
                EirError::InvalidElaborationExpression,
                request.range_expr.span(),
            ));
        };
        let start_value = self.elab_require_const_nat(start, env, "for range start")?;
        let end_value = self.elab_require_const_nat(end, env, "for range end")?;
        let (start_int, end_int) = match (start_value, end_value) {
            (ConstValue::Nat(start), ConstValue::Nat(end)) if start >= end => return Ok(Vec::new()),
            (ConstValue::Nat(start), ConstValue::Nat(end)) => (Some(start), Some(end)),
            _ => (None, None),
        };
        if let (Some(start), Some(end)) = (start_int, end_int) {
            return self.emit_static_for(request, env, start, end);
        }
        self.emit_symbolic_for(request, env, start, end)
    }

    fn emit_static_for(
        &self,
        request: ForEmit<'_>,
        env: &mut Env,
        start: u64,
        end: u64,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut items = Vec::new();
        let mut loop_env = env.clone();
        for value in start..end {
            loop_env.insert(
                request.name,
                EirExpr::Int(value),
                MirTypeRef::path_type(vec!["nat".to_string()], request.range_expr.span()),
            );
            items.extend(self.emit_for_iteration_body(
                request.body,
                &mut loop_env,
                request.name,
                value,
            )?);
        }
        self.sync_visible_software_locals(&loop_env, env);
        Ok(items)
    }

    fn emit_symbolic_for(
        &self,
        request: ForEmit<'_>,
        env: &mut Env,
        start: &crate::program::ElabExpr,
        end: &crate::program::ElabExpr,
    ) -> Result<Vec<EirItem>, CompileError> {
        let index = env.unique_label(request.name, request.span);
        let mut loop_env = env.clone();
        loop_env.insert(
            request.name,
            EirExpr::ident(&index),
            MirTypeRef::path_type(vec!["nat".to_string()], request.range_expr.span()),
        );
        let body_items =
            self.emit_symbolic_for_body(request.body, &mut loop_env, request.name, &index)?;
        self.sync_visible_software_locals(&loop_env, env);
        Ok(vec![EirItem::SymbolicStaticFor {
            index,
            start: self.elab_expr(start, env),
            end: self.elab_expr(end, env),
            label: env.unique_label(&format!("gen_{}", request.name), request.span),
            items: body_items,
            origin: env.origin(request.span),
        }])
    }

    fn emit_for_iteration_body(
        &self,
        body: &ElabBlock,
        env: &mut Env,
        loop_name: &str,
        index: u64,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut filtered = body.clone();
        let tail = filtered.tail.take();
        let mut items = self.emit_body_impl(&filtered, env, false)?;
        if let Some(tail) = tail
            && let ElabExprNode::Place {
                callee,
                args,
                inplace,
            } = &tail.node
        {
            let inst_name = env
                .prefix
                .as_deref()
                .map(|prefix| prefix.replace(&format!("[{loop_name}]"), &format!("[{index}]")))
                .unwrap_or_else(|| format!("place_{index}"));
            items.extend(self.emit_instance(InstanceEmitRequest {
                inst_name: &inst_name,
                callee,
                args,
                env,
                inplace: *inplace,
                span: tail.span(),
            })?);
        }
        Ok(items)
    }

    fn emit_symbolic_for_body(
        &self,
        body: &ElabBlock,
        env: &mut Env,
        loop_name: &str,
        index: &str,
    ) -> Result<Vec<EirItem>, CompileError> {
        let mut filtered = body.clone();
        let tail = filtered.tail.take();
        let mut items = self.emit_body_impl(&filtered, env, true)?;
        if let Some(tail) = tail
            && let ElabExprNode::Place {
                callee,
                args,
                inplace,
            } = &tail.node
        {
            let inst_name = env
                .prefix
                .as_deref()
                .map(|prefix| prefix.replace(&format!("[{loop_name}]"), &format!("[{index}]")))
                .unwrap_or_else(|| format!("place_{index}"));
            items.extend(self.emit_instance(InstanceEmitRequest {
                inst_name: &inst_name,
                callee,
                args,
                env,
                inplace: *inplace,
                span: tail.span(),
            })?);
        }
        Ok(items)
    }
}
