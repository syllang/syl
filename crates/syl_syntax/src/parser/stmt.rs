//! Statement and block-body parsing.
//!
//! Type-expression helpers live in `type_expr.rs`; aggregate field lists for
//! expressions live in `expr.rs`.

use super::{BlockContext, BlockEntry, Parser};
use crate::lexer::TokenKind;
use crate::{Block, Expr, RegReset, Stmt, TypeExpr};
use std::collections::HashSet;
use syl_span::Diagnostic;

impl Parser {

    pub(super) fn parse_block(&mut self, context: BlockContext) -> Result<Block, Vec<Diagnostic>> {
        let previous_context = self.block_context;
        self.block_context = context;
        self.mutable_local_scopes.push(HashSet::new());
        let result = (|| {
            let start = self.expect(TokenKind::LBrace)?.span;
            let mut stmts = Vec::new();
            let mut tail = None;
            while !self.check(&TokenKind::RBrace) && !self.is_eof() {
                let start_pos = self.pos;
                match self.parse_block_entry(context) {
                    Ok(BlockEntry::Stmt(stmt)) => stmts.push(*stmt),
                    Ok(BlockEntry::Tail(expr)) => {
                        tail = Some(Box::new(expr));
                        break;
                    }
                    Err(mut diagnostics) => {
                        self.diagnostics.append(&mut diagnostics);
                        let span = self.recover_stmt_boundary(start_pos);
                        stmts.push(Stmt::Error { span });
                    }
                }
            }
            let end = if let Some(tok) = self.consume(&TokenKind::RBrace) {
                tok.span
            } else {
                let span = self.eof_span();
                self.error(span, "expected RBrace");
                span
            };
            Ok(Block::new(stmts, tail, start.join(end)))
        })();
        let _ = self.mutable_local_scopes.pop();
        self.block_context = previous_context;
        result
    }

    fn parse_block_entry(&mut self, context: BlockContext) -> Result<BlockEntry, Vec<Diagnostic>> {
        if self.check(&TokenKind::KwLet) {
            return self
                .parse_let_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwConst) {
            return self
                .parse_const_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwVar) {
            return self
                .parse_var_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwSignal) {
            return self
                .parse_signal_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwReg) {
            return self
                .parse_reg_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwNext) {
            return self
                .parse_next_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwWhile) {
            return self
                .parse_while_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwFor) {
            return self
                .parse_for_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwIf) {
            return self
                .parse_if_stmt()
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.check(&TokenKind::KwReturn) {
            let span = self.expect(TokenKind::KwReturn)?.span;
            let expr = if self.check(&TokenKind::Semi) {
                None
            } else {
                Some(self.parse_expr(0)?)
            };
            let end = self
                .consume(&TokenKind::Semi)
                .map(|token| token.span)
                .unwrap_or_else(|| expr.as_ref().map(|expr| expr.span()).unwrap_or(span));
            return Ok(BlockEntry::Stmt(Box::new(Stmt::Return(
                expr,
                span.join(end),
            ))));
        }
        let expr = self.parse_expr(0)?;
        if matches!(
            self.peek_kind(),
            Some(TokenKind::Eq) | Some(TokenKind::ColonEq)
        ) {
            return self
                .parse_contextual_assignment_stmt(expr, context)
                .map(|stmt| BlockEntry::Stmt(Box::new(stmt)));
        }
        if self.consume(&TokenKind::Semi).is_some() || !self.check(&TokenKind::RBrace) {
            Ok(BlockEntry::Stmt(Box::new(Stmt::Expr(expr))))
        } else {
            Ok(BlockEntry::Tail(expr))
        }
    }

    pub(super) fn parse_let_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwLet)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let value = if self.consume(&TokenKind::Eq).is_some() {
            Some(self.parse_expr(0)?)
        } else if let Some(span) = self.consume(&TokenKind::ColonEq).map(|token| token.span) {
            self.error(span, "`let` statements only accept `=`");
            let _ = self.parse_expr(0)?;
            return Err(std::mem::take(&mut self.diagnostics));
        } else {
            None
        };
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| {
                value
                    .as_ref()
                    .map(Expr::span)
                    .or_else(|| ty.as_ref().map(TypeExpr::span))
                    .unwrap_or(start)
            });
        self.shadow_visible_mutable_local(&name);
        Ok(Stmt::Let {
            name,
            ty,
            value,
            span: start.join(end),
        })
    }

    pub(super) fn parse_const_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwConst)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr(0)?;
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| value.span());
        self.shadow_visible_mutable_local(&name);
        Ok(Stmt::Const {
            name,
            ty,
            value,
            span: start.join(end),
        })
    }


    pub(super) fn parse_var_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwVar)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let value = if self.consume(&TokenKind::Eq).is_some() {
            Some(self.parse_expr(0)?)
        } else if let Some(span) = self.consume(&TokenKind::ColonEq).map(|token| token.span) {
            self.error(span, "`var` statements only accept `=`");
            let _ = self.parse_expr(0)?;
            return Err(std::mem::take(&mut self.diagnostics));
        } else {
            None
        };
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| {
                value
                    .as_ref()
                    .map(Expr::span)
                    .or_else(|| ty.as_ref().map(TypeExpr::span))
                    .unwrap_or(start)
            });
        self.declare_mutable_local(&name);
        Ok(Stmt::Var {
            name,
            ty,
            value,
            span: start.join(end),
        })
    }

    pub(super) fn parse_signal_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwSignal)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let value = if self.consume(&TokenKind::ColonEq).is_some() {
            Some(self.parse_expr(0)?)
        } else if let Some(span) = self.consume(&TokenKind::Eq).map(|token| token.span) {
            self.error(span, "`signal` statements only accept `:=`");
            let _ = self.parse_expr(0)?;
            return Err(std::mem::take(&mut self.diagnostics));
        } else {
            None
        };
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| {
                value
                    .as_ref()
                    .map(Expr::span)
                    .or_else(|| ty.as_ref().map(TypeExpr::span))
                    .unwrap_or(start)
            });
        self.shadow_visible_mutable_local(&name);
        Ok(Stmt::Signal {
            name,
            ty,
            value,
            span: start.join(end),
        })
    }

    pub(super) fn parse_reg_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwReg)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let reset = if matches!(self.peek_kind(), Some(TokenKind::Ident(s)) if s == "reset") {
            Some(self.parse_reg_reset()?)
        } else {
            None
        };
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| {
                reset
                    .as_ref()
                    .map(|reset| reset.span)
                    .or_else(|| ty.as_ref().map(TypeExpr::span))
                    .unwrap_or(start)
            });
        self.shadow_visible_mutable_local(&name);
        Ok(Stmt::Reg {
            name,
            ty,
            reset,
            span: start.join(end),
        })
    }

    pub(super) fn parse_reg_reset(&mut self) -> Result<RegReset, Vec<Diagnostic>> {
        let start = self
            .expect_ident()
            .map(|_| self.prev_span())
            .unwrap_or_default();
        self.expect(TokenKind::LParen)?;
        let domain = Some(self.parse_expr(0)?);
        self.expect(TokenKind::Comma)?;
        let value = self.parse_expr(0)?;
        let end = self.expect(TokenKind::RParen)?.span;
        Ok(RegReset::new(domain, value, start.join(end)))
    }

    pub(super) fn parse_next_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwNext)?.span;
        let name = self.expect_ident()?;
        if self.consume(&TokenKind::ColonEq).is_none() {
            if let Some(span) = self.consume(&TokenKind::Eq).map(|token| token.span) {
                self.error(span, "`next` statements only accept `:=`");
                let _ = self.parse_expr(0)?;
                return Err(std::mem::take(&mut self.diagnostics));
            }
            self.expect(TokenKind::ColonEq)?;
        }
        let value = self.parse_expr(0)?;
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| value.span());
        Ok(Stmt::Next {
            name,
            value,
            span: start.join(end),
        })
    }

    pub(super) fn parse_contextual_assignment_stmt(
        &mut self,
        target: Expr,
        context: BlockContext,
    ) -> Result<Stmt, Vec<Diagnostic>> {
        let Some(operator) = self.bump() else {
            self.error(self.eof_span(), "expected assignment operator");
            return Err(std::mem::take(&mut self.diagnostics));
        };
        let is_hardware_mutable_local_assign = context == BlockContext::Hardware
            && matches!(operator.kind, TokenKind::Eq)
            && self.assignment_target_is_mutable_local(&target);
        let is_valid = match (context, operator.kind) {
            (BlockContext::Function, TokenKind::Eq) => true,
            (BlockContext::Hardware, TokenKind::ColonEq) => true,
            _ if is_hardware_mutable_local_assign => true,
            (BlockContext::Function, TokenKind::ColonEq) => {
                self.error(
                    operator.span,
                    "`fn` blocks use `=`; `:=` is only valid in hardware blocks",
                );
                false
            }
            (BlockContext::Hardware, TokenKind::Eq) => {
                self.error(
                    operator.span,
                    "hardware blocks use `:=`; bare `=` assignment is invalid here",
                );
                false
            }
            _ => {
                self.error(operator.span, "expected assignment operator");
                false
            }
        };
        let value = self.parse_expr(0)?;
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| value.span());
        let span = target.span().join(end);
        if !is_valid {
            return Err(std::mem::take(&mut self.diagnostics));
        }
        match context {
            BlockContext::Function => Ok(Stmt::Assign {
                target,
                value,
                span,
            }),
            BlockContext::Hardware if is_hardware_mutable_local_assign => Ok(Stmt::Assign {
                target,
                value,
                span,
            }),
            BlockContext::Hardware => Ok(Stmt::Drive {
                target,
                value,
                span,
            }),
        }
    }

    fn assignment_target_is_mutable_local(&self, target: &Expr) -> bool {
        self.assignment_target_root_ident(target)
            .is_some_and(|name| self.is_mutable_local(name))
    }

    fn assignment_target_root_ident<'a>(&self, target: &'a Expr) -> Option<&'a str> {
        let mut cursor = target;
        loop {
            match cursor {
                Expr::Ident(name, _) => return Some(name.as_str()),
                Expr::Field { base, .. } | Expr::Index { base, .. } | Expr::Group(base, _) => {
                    cursor = base.as_ref();
                }
                _ => return None,
            }
        }
    }

    pub(super) fn parse_while_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwWhile)?.span;
        let cond = self.parse_expr(0)?;
        let body = self.parse_nested_block_preserving_mutable_scope(self.block_context())?;
        let span = start.join(body.span);
        Ok(Stmt::While { cond, body, span })
    }

    pub(super) fn parse_if_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwIf)?.span;
        let cond = self.parse_expr(0)?;
        let then_block = self.parse_nested_block_preserving_mutable_scope(self.block_context())?;
        let else_block = if self.check(&TokenKind::KwElse) {
            self.expect(TokenKind::KwElse)?;
            Some(self.parse_nested_block_preserving_mutable_scope(self.block_context())?)
        } else {
            None
        };
        let end = else_block
            .as_ref()
            .map(|b| b.span)
            .unwrap_or(then_block.span);
        Ok(Stmt::ElabIf {
            cond,
            then_block,
            else_block,
            span: start.join(end),
        })
    }

    pub(super) fn parse_for_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwFor)?.span;
        let name = self.expect_ident()?;
        if self.consume(&TokenKind::KwIn).is_none() {
            self.error(start, "expected `in` after `for` loop variable");
            return Err(std::mem::take(&mut self.diagnostics));
        }
        let start_expr = self.parse_expr(0)?;
        self.expect(TokenKind::DotDot)?;
        let end_expr = self.parse_expr(0)?;
        let span = start_expr.span().join(end_expr.span());
        let range = Expr::Range {
            start: Box::new(start_expr),
            end: Box::new(end_expr),
            span,
        };
        let saved_scopes = self.mutable_local_scopes.clone();
        self.shadow_visible_mutable_local(&name);
        let body = self.parse_block(self.block_context());
        self.mutable_local_scopes = saved_scopes;
        let body = body?;
        let span = start.join(body.span);
        Ok(Stmt::ElabFor {
            name,
            range,
            body,
            span,
        })
    }


    fn parse_nested_block_preserving_mutable_scope(
        &mut self,
        context: BlockContext,
    ) -> Result<crate::Block, Vec<Diagnostic>> {
        let saved_scopes = self.mutable_local_scopes.clone();
        let block = self.parse_block(context);
        self.mutable_local_scopes = saved_scopes;
        block
    }

    fn shadow_visible_mutable_local(&mut self, name: &str) {
        for scope in self.mutable_local_scopes.iter_mut().rev() {
            if scope.remove(name) {
                break;
            }
        }
    }


}
