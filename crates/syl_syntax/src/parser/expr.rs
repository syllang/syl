use super::Parser;
use super::span_ext::PatternSpan;
use crate::lexer::{Token, TokenKind};
use crate::*;
use syl_span::{Diagnostic, Span};

impl Parser {
    pub(super) fn looks_like_generic_app(&self) -> bool {
        let mut depth = 0usize;
        let mut idx = self.pos;
        while let Some(tok) = self.tokens.get(idx) {
            match tok.kind {
                TokenKind::Lt => depth += 1,
                TokenKind::Gt => {
                    if depth == 0 {
                        return false;
                    }
                    depth -= 1;
                    if depth == 0 {
                        return matches!(
                            self.tokens.get(idx + 1).map(|t| &t.kind),
                            Some(TokenKind::LParen | TokenKind::LBrace)
                        );
                    }
                }
                _ => {}
            }
            idx += 1;
        }
        false
    }

    pub(super) fn parse_match(&mut self, start: Span) -> Result<Expr, Vec<Diagnostic>> {
        let expr = self.parse_expr(0)?;
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let doc = self.take_doc_for_next_token();
            let pattern = self.parse_pattern()?;
            if self.consume(&TokenKind::EqGt).is_none() {
                self.expect(TokenKind::Arrow)?;
            }
            let value = self.parse_expr(0)?;
            let span = PatternSpan::new(&pattern).span().join(value.span());
            let mut arm = MatchArm::new(pattern, value, span);
            arm.doc = doc;
            arms.push(arm);
            self.consume(&TokenKind::Comma);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok(Expr::Match {
            expr: Box::new(expr),
            arms,
            span: start.join(end),
        })
    }

    pub(super) fn parse_select_expr(&mut self, start: Span) -> Result<Expr, Vec<Diagnostic>> {
        let mode = if self.consume(&TokenKind::KwPriority).is_some() {
            SelectMode::Priority
        } else if self.consume(&TokenKind::KwUnique).is_some() {
            SelectMode::Unique
        } else {
            SelectMode::Priority
        };
        self.expect(TokenKind::LBrace)?;
        let mut arms = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let doc = self.take_doc_for_next_token();
            let pattern = self.parse_expr(0)?;
            if self.consume(&TokenKind::EqGt).is_none() {
                self.expect(TokenKind::Arrow)?;
            }
            let value = self.parse_expr(0)?;
            let span = pattern.span().join(value.span());
            let mut arm = SelectArm::new(pattern, value, span);
            arm.doc = doc;
            arms.push(arm);
            self.consume(&TokenKind::Comma);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok(Expr::Select {
            mode,
            arms,
            span: start.join(end),
        })
    }

    pub(super) fn parse_place_expr(&mut self, start: Span) -> Result<Expr, Vec<Diagnostic>> {
        self.parse_place_expr_with_inplace(start, false)
    }

    pub(super) fn parse_inplace_expr(&mut self, start: Span) -> Result<Expr, Vec<Diagnostic>> {
        self.parse_place_expr_with_inplace(start, true)
    }

    fn parse_place_expr_with_inplace(
        &mut self,
        start: Span,
        inplace: bool,
    ) -> Result<Expr, Vec<Diagnostic>> {
        let mut callee = self.parse_prefix()?;
        while self.peek_kind() == Some(&TokenKind::Lt) && self.looks_like_generic_app() {
            self.bump();
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
            let span = callee.span().join(end);
            callee = Expr::GenericApp {
                callee: Box::new(callee),
                args,
                span,
            };
        }
        let mut args = Vec::new();
        let mut end = callee.span();
        if self.consume(&TokenKind::LParen).is_some() {
            if !self.check(&TokenKind::RParen) {
                loop {
                    let arg_start = self.peek().map(|t| t.span).unwrap_or_default();
                    let arg = if matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
                        && matches!(
                            self.tokens.get(self.pos + 1).map(|t| &t.kind),
                            Some(TokenKind::Colon)
                        ) {
                        let name = self.expect_ident()?;
                        self.expect(TokenKind::Colon)?;
                        let value = self.parse_expr(0)?;
                        let span = arg_start.join(value.span());
                        CallArg::new(Some(name), value, span)
                    } else {
                        let value = self.parse_expr(0)?;
                        let span = value.span();
                        CallArg::new(None, value, span)
                    };
                    args.push(arg);
                    if self.consume(&TokenKind::Comma).is_none() || self.check(&TokenKind::RParen) {
                        break;
                    }
                }
            }
            end = self.expect(TokenKind::RParen)?.span;
        }
        let span = start.join(end);
        Ok(Expr::Place {
            callee: Box::new(callee),
            args,
            inplace,
            span,
        })
    }

    pub(super) fn parse_for_expr(&mut self, start: Span) -> Result<Expr, Vec<Diagnostic>> {
        let name = self.expect_ident()?;
        if self.consume(&TokenKind::KwIn).is_none() {
            self.error(start, "expected `in` after `for` loop variable");
            return Err(std::mem::take(&mut self.diagnostics));
        }
        let start_expr = self.parse_expr(0)?;
        self.expect(TokenKind::DotDot)?;
        let end_expr = self.parse_expr(0)?;
        let range_span = start_expr.span().join(end_expr.span());
        let range = Expr::Range {
            start: Box::new(start_expr),
            end: Box::new(end_expr),
            span: range_span,
        };
        let body = self.parse_block(self.block_context())?;
        let span = start.join(body.span);
        Ok(Expr::For {
            name,
            range: Box::new(range),
            body,
            span,
        })
    }

    pub(super) fn parse_compile_error(&mut self, start: Span) -> Result<Expr, Vec<Diagnostic>> {
        self.expect(TokenKind::LParen)?;
        let message = if self.check(&TokenKind::RParen) {
            self.error(start, "compile_error requires a message");
            return Err(std::mem::take(&mut self.diagnostics));
        } else {
            self.parse_expr(0)?
        };
        let end = self.expect(TokenKind::RParen)?.span;
        Ok(Expr::CompileError {
            message: Box::new(message),
            span: start.join(end),
        })
    }

    pub(super) fn parse_pattern(&mut self) -> Result<Pattern, Vec<Diagnostic>> {
        match self.bump() {
            Some(Token {
                kind: TokenKind::Ident(name),
                span,
                ..
            }) if name == "_" => Ok(Pattern::Wildcard(span)),
            Some(Token {
                kind: TokenKind::Ident(name),
                span,
                ..
            }) => Ok(Pattern::Ident(name, span)),
            Some(Token {
                kind: TokenKind::Dot,
                span,
                ..
            }) => {
                let mut path = vec![self.expect_ident()?];
                while self.consume(&TokenKind::Dot).is_some() {
                    path.push(self.expect_ident()?);
                }
                let end = self.prev_span();
                Ok(Pattern::Path(path, span.join(end)))
            }
            Some(Token {
                kind: TokenKind::Int(v),
                span,
                ..
            }) => Ok(Pattern::Int(v, span)),
            Some(Token {
                kind: TokenKind::Bool(v),
                span,
                ..
            }) => Ok(Pattern::Bool(v, span)),
            Some(tok) => {
                self.error(tok.span, "expected pattern");
                Err(std::mem::take(&mut self.diagnostics))
            }
            None => {
                self.error(self.eof_span(), "unexpected end of source");
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }

    pub(super) fn parse_expr(&mut self, min_bp: u8) -> Result<Expr, Vec<Diagnostic>> {
        let mut lhs = self.parse_prefix()?;
        loop {
            if self.peek_kind() == Some(&TokenKind::Lt) && self.looks_like_generic_app() {
                self.bump();
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
                let span = lhs.span().join(end);
                lhs = Expr::GenericApp {
                    callee: Box::new(lhs),
                    args,
                    span,
                };
                continue;
            }
            if self.consume(&TokenKind::LParen).is_some() {
                let mut args = Vec::new();
                if !self.check(&TokenKind::RParen) {
                    loop {
                        let arg_start = self.peek().map(|t| t.span).unwrap_or_default();
                        let arg = if matches!(self.peek_kind(), Some(TokenKind::Ident(_)))
                            && matches!(
                                self.tokens.get(self.pos + 1).map(|t| &t.kind),
                                Some(TokenKind::Colon)
                            ) {
                            let name = self.expect_ident()?;
                            self.expect(TokenKind::Colon)?;
                            let value = self.parse_expr(0)?;
                            let span = arg_start.join(value.span());
                            CallArg::new(Some(name), value, span)
                        } else {
                            let value = self.parse_expr(0)?;
                            let span = value.span();
                            CallArg::new(None, value, span)
                        };
                        args.push(arg);
                        if self.consume(&TokenKind::Comma).is_none()
                            || self.check(&TokenKind::RParen)
                        {
                            break;
                        }
                    }
                }
                let end = self.expect(TokenKind::RParen)?.span;
                let span = lhs.span().join(end);
                lhs = Expr::Call {
                    callee: Box::new(lhs),
                    args,
                    span,
                };
                continue;
            }
            if self.peek_kind() == Some(&TokenKind::LBrace) && self.looks_like_aggregate() {
                self.bump();
                let fields = self.parse_named_fields()?;
                let end = self.expect(TokenKind::RBrace)?.span;
                let ty = self.expr_to_type_expr(lhs.clone())?;
                let span = ty.span().join(end);
                lhs = Expr::Aggregate {
                    ty: Box::new(ty),
                    fields,
                    span,
                };
                continue;
            }
            if self.consume(&TokenKind::LBracket).is_some() {
                let index = self.parse_expr(0)?;
                let end = self.expect(TokenKind::RBracket)?.span;
                let span = lhs.span().join(end);
                lhs = Expr::Index {
                    base: Box::new(lhs),
                    index: Box::new(index),
                    span,
                };
                continue;
            }
            if self.consume(&TokenKind::Dot).is_some() {
                let field = self.expect_ident()?;
                let span = lhs.span().join(self.prev_span());
                lhs = Expr::Field {
                    base: Box::new(lhs),
                    field,
                    span,
                };
                continue;
            }

            let (l_bp, r_bp, op) = match self.peek_kind() {
                Some(TokenKind::OrOr) => (1, 2, BinaryOp::OrOr),
                Some(TokenKind::KwOr) => (1, 2, BinaryOp::OrWord),
                Some(TokenKind::AndAnd) => (3, 4, BinaryOp::AndAnd),
                Some(TokenKind::KwAnd) => (3, 4, BinaryOp::AndWord),
                Some(TokenKind::EqEq) => (5, 6, BinaryOp::EqEq),
                Some(TokenKind::KwEqWord) => (5, 6, BinaryOp::EqWord),
                Some(TokenKind::BangEq) => (5, 6, BinaryOp::NotEq),
                Some(TokenKind::Lt) => (7, 8, BinaryOp::Lt),
                Some(TokenKind::LtEq) => (7, 8, BinaryOp::LtEq),
                Some(TokenKind::Gt) => (7, 8, BinaryOp::Gt),
                Some(TokenKind::GtEq) => (7, 8, BinaryOp::GtEq),
                Some(TokenKind::Plus) => (9, 10, BinaryOp::Add),
                Some(TokenKind::Minus) => (9, 10, BinaryOp::Sub),
                Some(TokenKind::KwXor) => (9, 10, BinaryOp::XorWord),
                Some(TokenKind::LtLt) => (11, 12, BinaryOp::Shl),
                Some(TokenKind::Star) => (11, 12, BinaryOp::Mul),
                Some(TokenKind::Slash) => (11, 12, BinaryOp::Div),
                Some(TokenKind::Percent) => (11, 12, BinaryOp::Rem),
                _ => break,
            };
            if l_bp < min_bp {
                break;
            }
            let op_span = match self.bump() {
                Some(tok) => tok.span,
                None => {
                    self.error(
                        self.eof_span(),
                        "operator token disappeared during Pratt parse",
                    );
                    return Err(std::mem::take(&mut self.diagnostics));
                }
            };
            let rhs = self.parse_expr(r_bp)?;
            let span = lhs.span().join(rhs.span()).join(op_span);
            lhs = Expr::Binary {
                op,
                left: Box::new(lhs),
                right: Box::new(rhs),
                span,
            };
        }
        Ok(lhs)
    }

    pub(super) fn parse_prefix(&mut self) -> Result<Expr, Vec<Diagnostic>> {
        match self.bump() {
            Some(Token {
                kind: TokenKind::Ident(name),
                span,
                ..
            }) if name == "compile_error"
                || (name == "error" && self.check(&TokenKind::LParen)) =>
            {
                self.parse_compile_error(span)
            }
            Some(Token {
                kind: TokenKind::Ident(name),
                span,
                ..
            }) => Ok(Expr::Ident(name, span)),
            Some(Token {
                kind: TokenKind::Int(value),
                span,
                ..
            }) => Ok(Expr::Int(value, span)),
            Some(Token {
                kind: TokenKind::Str(value),
                span,
                ..
            }) => Ok(Expr::Str(value, span)),
            Some(Token {
                kind: TokenKind::Bool(value),
                span,
                ..
            }) => Ok(Expr::Bool(value, span)),
            Some(Token {
                kind: TokenKind::Minus,
                span,
                ..
            }) => {
                let expr = self.parse_expr(12)?;
                let span = span.join(expr.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                    span,
                })
            }
            Some(Token {
                kind: TokenKind::Bang,
                span,
                ..
            }) => {
                let expr = self.parse_expr(12)?;
                let span = span.join(expr.span());
                Ok(Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                    span,
                })
            }
            Some(Token {
                kind: TokenKind::KwNot,
                span,
                ..
            }) => {
                let expr = self.parse_expr(12)?;
                let span = span.join(expr.span());
                Ok(Expr::Unary {
                    op: UnaryOp::NotWord,
                    expr: Box::new(expr),
                    span,
                })
            }
            Some(Token {
                kind: TokenKind::LParen,
                span,
            }) => {
                let expr = self.parse_expr(0)?;
                let end = self.expect(TokenKind::RParen)?.span;
                Ok(Expr::Group(Box::new(expr), span.join(end)))
            }
            Some(Token {
                kind: TokenKind::LBrace,
                span: _,
                ..
            }) => {
                self.pos -= 1;
                let block = self.parse_block(self.block_context())?;
                Ok(Expr::Block(block))
            }
            Some(Token {
                kind: TokenKind::KwMatch,
                span,
                ..
            }) => self.parse_match(span),
            Some(Token {
                kind: TokenKind::KwSelect,
                span,
                ..
            }) => self.parse_select_expr(span),
            Some(Token {
                kind: TokenKind::KwPlace,
                span,
                ..
            }) => self.parse_place_expr(span),
            Some(Token {
                kind: TokenKind::KwInplace,
                span,
                ..
            }) => self.parse_inplace_expr(span),
            Some(Token {
                kind: TokenKind::KwFor,
                span,
                ..
            }) => self.parse_for_expr(span),
            Some(tok) => {
                self.error(tok.span, "expected expression");
                Err(std::mem::take(&mut self.diagnostics))
            }
            None => {
                self.error(self.eof_span(), "unexpected end of source");
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }
}
