use super::Parser;
use crate::lexer::{Token, TokenKind};
use crate::*;
use syl_span::{Diagnostic, Span};

impl Parser {
    pub(super) fn parse_type_expr(&mut self) -> Result<TypeExpr, Vec<Diagnostic>> {
        let mut ty = self.parse_type_prefix()?;
        loop {
            if self.consume(&TokenKind::Dot).is_none() {
                break;
            }
            let view = self.expect_ident()?;
            let span = ty.span().join(self.prev_span());
            ty = TypeExpr::ViewSelect {
                base: Box::new(ty),
                view,
                span,
            };
        }
        Ok(ty)
    }

    pub(super) fn parse_generic_params(&mut self) -> Result<Vec<GenericParam>, Vec<Diagnostic>> {
        let mut params = Vec::new();
        if self.consume(&TokenKind::Lt).is_some() {
            if !self.check(&TokenKind::Gt) {
                loop {
                    let doc = self.take_doc_for_next_token();
                    let name = self.expect_ident()?;
                    let start = self.prev_span();
                    let kind = if self.consume(&TokenKind::Colon).is_some() {
                        Some(self.parse_type_expr()?)
                    } else {
                        None
                    };
                    let default = if self.consume(&TokenKind::Eq).is_some() {
                        Some(self.parse_expr(0)?)
                    } else {
                        None
                    };
                    let end = default
                        .as_ref()
                        .map(Expr::span)
                        .or_else(|| kind.as_ref().map(TypeExpr::span))
                        .unwrap_or(start);
                    let mut param = GenericParam::new(name, kind, default, start.join(end));
                    param.doc = doc;
                    params.push(param);
                    if self.consume(&TokenKind::Comma).is_none() {
                        break;
                    }
                    if self.check(&TokenKind::Gt) {
                        break;
                    }
                }
            }
            self.expect(TokenKind::Gt)?;
        }
        Ok(params)
    }

    pub(super) fn parse_param_list(&mut self) -> Result<Vec<Param>, Vec<Diagnostic>> {
        let mut params = Vec::new();
        self.expect(TokenKind::LParen)?;
        if !self.check(&TokenKind::RParen) {
            loop {
                let doc = self.take_doc_for_next_token();
                let receiver = self.consume(&TokenKind::KwThis).is_some();
                if receiver && !params.is_empty() {
                    self.error(
                        self.prev_span(),
                        "`this` receiver must be the first parameter",
                    );
                    return Err(std::mem::take(&mut self.diagnostics));
                }
                let name = self.expect_ident()?;
                let start = self.prev_span();
                self.expect(TokenKind::Colon)?;
                let dir = if self.consume(&TokenKind::KwIn).is_some() {
                    Some(ParamDirection::In)
                } else if self.consume(&TokenKind::KwInOut).is_some() {
                    Some(ParamDirection::InOut)
                } else if self.consume(&TokenKind::KwOut).is_some() {
                    Some(ParamDirection::Out)
                } else {
                    None
                };
                let ty = self.parse_type_expr()?;
                let span = start.join(ty.span());
                if receiver {
                    if dir.is_some() {
                        self.error(span, "`this` receiver cannot have an in/out direction");
                        return Err(std::mem::take(&mut self.diagnostics));
                    }
                    let mut param = Param::receiver(name, ty, span);
                    param.doc = doc;
                    params.push(param);
                } else {
                    let mut param = Param::new(name, dir, ty, span);
                    param.doc = doc;
                    params.push(param);
                }
                if self.consume(&TokenKind::Comma).is_none() {
                    break;
                }
                if self.check(&TokenKind::RParen) {
                    break;
                }
            }
        }
        self.expect(TokenKind::RParen)?;
        Ok(params)
    }

    pub(super) fn parse_result_binding(&mut self) -> Result<ResultBinding, Vec<Diagnostic>> {
        let doc = self.take_doc_for_next_token();
        let name = self.expect_ident()?;
        let start = self.prev_span();
        self.expect(TokenKind::Colon)?;
        let ty = self.parse_type_expr()?;
        let span = start.join(ty.span());
        let mut result = ResultBinding::new(name, ty, DriveCapability::WriteOnly, span);
        result.doc = doc;
        Ok(result)
    }

    pub(super) fn parse_field_block(&mut self) -> Result<(Vec<FieldDecl>, Span), Vec<Diagnostic>> {
        let start = self.expect(TokenKind::LBrace)?.span;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let doc = self.take_doc_for_next_token();
            let name = self.expect_ident()?;
            let field_start = self.prev_span();
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            let span = field_start.join(ty.span());
            let mut field = FieldDecl::new(name, ty, span);
            field.doc = doc;
            fields.push(field);
            self.consume(&TokenKind::Comma);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok((fields, start.join(end)))
    }

    pub(super) fn parse_interface_body(
        &mut self,
    ) -> Result<(Vec<FieldDecl>, Vec<ViewDecl>, Span), Vec<Diagnostic>> {
        let start = self.expect(TokenKind::LBrace)?.span;
        let mut fields = Vec::new();
        let mut views = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            if self.check(&TokenKind::KwView) {
                views.push(self.parse_view_decl()?);
                continue;
            }
            let doc = self.take_doc_for_next_token();
            let name = self.expect_ident()?;
            let field_start = self.prev_span();
            self.expect(TokenKind::Colon)?;
            let ty = self.parse_type_expr()?;
            let span = field_start.join(ty.span());
            let mut field = FieldDecl::new(name, ty, span);
            field.doc = doc;
            fields.push(field);
            self.consume(&TokenKind::Comma);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok((fields, views, start.join(end)))
    }

    pub(super) fn parse_view_decl(&mut self) -> Result<ViewDecl, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwView)?.span;
        let name = self.expect_ident()?;
        self.expect(TokenKind::LBrace)?;
        let mut fields = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let doc = self.take_doc_for_next_token();
            let (dir, dir_span) = match self.bump() {
                Some(Token {
                    kind: TokenKind::KwIn,
                    span,
                    ..
                }) => (ViewDirection::In, span),
                Some(Token {
                    kind: TokenKind::KwInOut,
                    span,
                    ..
                }) => (ViewDirection::InOut, span),
                Some(Token {
                    kind: TokenKind::KwOut,
                    span,
                    ..
                }) => (ViewDirection::Out, span),
                Some(tok) => {
                    self.error(tok.span, "expected in, inout, or out");
                    return Err(std::mem::take(&mut self.diagnostics));
                }
                None => {
                    self.error(self.eof_span(), "unexpected end of source");
                    return Err(std::mem::take(&mut self.diagnostics));
                }
            };
            let field = self.expect_ident()?;
            let end = self.prev_span();
            let mut field = ViewField::new(dir, field, dir_span.join(end));
            field.doc = doc;
            fields.push(field);
            self.consume(&TokenKind::Comma);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok(ViewDecl::new(name, fields, start.join(end)))
    }
}
