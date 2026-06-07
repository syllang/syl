use std::collections::HashMap;

use crate::{
    CompileError, DriverError,
    eir::EirExpr,
    program::{ElabBlock, ElabExpr, ElabStmt},
};
use syl_span::Span;

use super::{EirBuilder, Env};

impl<'a, C> EirBuilder<'a, C>
where
    C: crate::const_eval::ConstValueElaborator + ?Sized,
{
    fn next_map<'b>(
        &self,
        body: &'b ElabBlock,
    ) -> Result<HashMap<String, (&'b ElabExpr, Span)>, CompileError> {
        let mut nexts = HashMap::new();
        for stmt in &body.stmts {
            if let ElabStmt::Next { name, value, span } = stmt
                && nexts.insert(name.clone(), (value, *span)).is_some()
            {
                return Err(CompileError::lowering_at(
                    DriverError::DuplicateNextDriver { name: name.clone() },
                    *span,
                ));
            }
        }
        Ok(nexts)
    }

    pub(super) fn next_expr(
        &self,
        name: &str,
        body: &ElabBlock,
        env: &Env,
    ) -> Result<Option<EirExpr>, CompileError> {
        let direct = self.next_map(body)?;
        let mut found: Option<(EirExpr, Span)> = None;
        if let Some((expr, span)) = direct.get(name) {
            found = Some((self.elab_expr(expr, env), *span));
        }
        for stmt in &body.stmts {
            if let ElabStmt::ElabIf {
                cond,
                then_block,
                else_block,
                span,
                ..
            } = stmt
            {
                let conditional = match self.elab_const_bool(cond, env)? {
                    Some(true) => self.next_expr(name, then_block, env)?,
                    Some(false) => else_block
                        .as_ref()
                        .map(|block| self.next_expr(name, block, env))
                        .transpose()?
                        .flatten(),
                    None => {
                        let then_next = self.next_expr(name, then_block, env)?;
                        let else_next = else_block
                            .as_ref()
                            .map(|block| self.next_expr(name, block, env))
                            .transpose()?
                            .flatten();
                        if then_next.is_some() || else_next.is_some() {
                            let hold = env
                                .vars
                                .get(name)
                                .map(|var| var.code.clone())
                                .unwrap_or_else(|| EirExpr::ident(name));
                            let then_code = then_next.unwrap_or_else(|| hold.clone());
                            let else_code = else_next.unwrap_or(hold);
                            Some(EirExpr::mux(
                                self.elab_expr(cond, env),
                                then_code,
                                else_code,
                            ))
                        } else {
                            None
                        }
                    }
                };
                if let Some(code) = conditional {
                    if found.is_some() {
                        return Err(CompileError::lowering_at(
                            DriverError::DuplicateNextDriver {
                                name: name.to_string(),
                            },
                            *span,
                        ));
                    }
                    found = Some((code, *span));
                }
            }
        }
        Ok(found.map(|(expr, _)| expr))
    }

    pub(super) fn next_reads(
        &self,
        name: &str,
        body: &ElabBlock,
        env: &Env,
    ) -> Result<Vec<EirExpr>, CompileError> {
        let direct = self.next_map(body)?;
        let mut reads = Vec::new();
        if let Some((expr, _)) = direct.get(name) {
            reads.extend(self.elab_read_places(expr, env));
        }
        for stmt in &body.stmts {
            if let ElabStmt::ElabIf {
                then_block,
                else_block,
                ..
            } = stmt
            {
                reads.extend(self.next_reads(name, then_block, env)?);
                if let Some(block) = else_block {
                    reads.extend(self.next_reads(name, block, env)?);
                }
            }
        }
        Ok(reads)
    }
}
