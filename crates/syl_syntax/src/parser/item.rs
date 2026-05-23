use super::Parser;
use crate::Attribute;
use std::vec::Vec;
use syl_span::Diagnostic;

impl Parser {
    pub(super) fn parse_attrs(&mut self) -> Result<Vec<Attribute>, Vec<Diagnostic>> {
        let mut attrs = Vec::new();
        while self.check(&crate::lexer::TokenKind::At) {
            let at = self.expect(crate::lexer::TokenKind::At)?.span;
            let name = self.expect_ident()?;
            let mut args = Vec::new();
            if self.consume(&crate::lexer::TokenKind::LParen).is_some() {
                if !self.check(&crate::lexer::TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr(0)?);
                        if self.consume(&crate::lexer::TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self.expect(crate::lexer::TokenKind::RParen)?.span;
                attrs.push(Attribute::new(name, args, at.join(end)));
            } else {
                attrs.push(Attribute::new(name, args, at));
            }
        }
        Ok(attrs)
    }

    pub(super) fn parse_path(&mut self) -> Result<Vec<String>, Vec<Diagnostic>> {
        let mut path = vec![self.expect_ident()?];
        while self.consume(&crate::lexer::TokenKind::Dot).is_some() {
            path.push(self.expect_ident()?);
        }
        Ok(path)
    }
}
