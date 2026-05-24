use super::Parser;
use crate::lexer::{Token, TokenKind};
use crate::{Expr, NamedExpr, RegReset, Stmt, TypeExpr};
use syl_span::Diagnostic;

impl Parser {
    pub(super) fn parse_let_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwLet)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let value = if self.consume(&TokenKind::Eq).is_some()
            || self.consume(&TokenKind::ColonEq).is_some()
        {
            Some(self.parse_expr(0)?)
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
        Ok(Stmt::Const {
            name,
            ty,
            value,
            span: start.join(end),
        })
    }

    pub(super) fn parse_alias_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwAlias)?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::Eq)?;
        let value = self.parse_expr(0)?;
        let span = start.join(value.span());
        Ok(Stmt::Alias { name, value, span })
    }

    pub(super) fn parse_type_prefix(&mut self) -> Result<TypeExpr, Vec<Diagnostic>> {
        if let Some(start) = self.consume(&TokenKind::LBracket).map(|token| token.span) {
            let len = self.parse_expr(0)?;
            let end = self.expect(TokenKind::RBracket)?.span;
            let elem = self.parse_type_expr()?;
            let span = start.join(end).join(elem.span());
            return Ok(TypeExpr::Array {
                len: Box::new(len),
                elem: Box::new(elem),
                span,
            });
        }

        let Some(tok) = self.bump() else {
            self.error(self.eof_span(), "unexpected end of source");
            return Err(std::mem::take(&mut self.diagnostics));
        };
        let start = tok.span;
        let mut parts = match tok.kind {
            TokenKind::Ident(name) => vec![name],
            TokenKind::Int(value) => vec![value.to_string()],
            TokenKind::Bool(value) => vec![if value {
                "true".to_string()
            } else {
                "false".to_string()
            }],
            _ => {
                self.error(tok.span, "expected type");
                return Err(std::mem::take(&mut self.diagnostics));
            }
        };
        while self.consume(&TokenKind::Dot).is_some() {
            parts.push(self.expect_ident()?);
        }
        let path_end = self.prev_span();
        let mut ty = TypeExpr::Path(parts, start.join(path_end));
        if self.consume(&TokenKind::Lt).is_some() {
            let mut args = Vec::new();
            if !self.check(&TokenKind::Gt) {
                loop {
                    args.push(self.parse_type_expr()?);
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                }
            }
            let end = self.expect(TokenKind::Gt)?.span;
            let span = start.join(end);
            ty = TypeExpr::Generic {
                base: Box::new(ty),
                args,
                span,
            };
        }
        Ok(ty)
    }

    pub(super) fn parse_var_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwVar)?.span;
        let name = self.expect_ident()?;
        let ty = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let value = if self.consume(&TokenKind::Eq).is_some()
            || self.consume(&TokenKind::ColonEq).is_some()
        {
            Some(self.parse_expr(0)?)
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
        let value = if self.consume(&TokenKind::Eq).is_some()
            || self.consume(&TokenKind::ColonEq).is_some()
        {
            Some(self.parse_expr(0)?)
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
        if self.consume(&TokenKind::Eq).is_none() && self.consume(&TokenKind::ColonEq).is_none() {
            self.expect(TokenKind::Eq)?;
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

    pub(super) fn parse_inst_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwInst)?.span;
        let name = self.parse_expr(1)?;
        if self.consume(&TokenKind::Eq).is_none() && self.consume(&TokenKind::ColonEq).is_none() {
            self.expect(TokenKind::Eq)?;
        }
        let callee = self.parse_expr(0)?;
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| callee.span());
        Ok(Stmt::Inst {
            name,
            callee,
            span: start.join(end),
        })
    }

    pub(super) fn parse_while_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwWhile)?.span;
        let cond = self.parse_expr(0)?;
        let body = self.parse_block()?;
        let span = start.join(body.span);
        Ok(Stmt::While { cond, body, span })
    }

    pub(super) fn parse_if_stmt(&mut self) -> Result<Stmt, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwIf)?.span;
        let cond = self.parse_expr(0)?;
        let then_block = self.parse_block()?;
        let else_block = if self.check(&TokenKind::KwElse) {
            self.expect(TokenKind::KwElse)?;
            Some(self.parse_block()?)
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
        let body = self.parse_block()?;
        let span = start.join(body.span);
        Ok(Stmt::ElabFor {
            name,
            range,
            body,
            span,
        })
    }

    pub(super) fn parse_named_fields(&mut self) -> Result<Vec<NamedExpr>, Vec<Diagnostic>> {
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let name = self.expect_ident()?;
            let start = self.prev_span();
            self.expect(TokenKind::Colon)?;
            let value = self.parse_expr(0)?;
            let span = start.join(value.span());
            fields.push(NamedExpr::new(name, value, span));
            self.consume(&TokenKind::Comma);
        }
        Ok(fields)
    }

    pub(super) fn looks_like_aggregate(&self) -> bool {
        matches!(
            self.tokens.get(self.pos),
            Some(Token {
                kind: TokenKind::LBrace,
                ..
            })
        ) && matches!(
            self.tokens.get(self.pos + 1).map(|t| &t.kind),
            Some(TokenKind::Ident(_))
        ) && matches!(
            self.tokens.get(self.pos + 2).map(|t| &t.kind),
            Some(TokenKind::Colon)
        )
    }

    pub(super) fn expr_to_type_expr(&mut self, expr: Expr) -> Result<TypeExpr, Vec<Diagnostic>> {
        match expr {
            Expr::Ident(name, span) => Ok(TypeExpr::Path(vec![name], span)),
            Expr::GenericApp { callee, args, span } => {
                let base = self.expr_to_type_expr(*callee)?;
                Ok(TypeExpr::Generic {
                    base: Box::new(base),
                    args,
                    span,
                })
            }
            Expr::Field { base, field, span } => {
                let base = self.expr_to_type_expr(*base)?;
                match base {
                    TypeExpr::Path(mut path, base_span) => {
                        path.push(field);
                        Ok(TypeExpr::Path(path, base_span.join(span)))
                    }
                    _ => {
                        self.error(span, "invalid aggregate type");
                        Err(std::mem::take(&mut self.diagnostics))
                    }
                }
            }
            other => {
                self.error(other.span(), "expected type-like expression");
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }
}
