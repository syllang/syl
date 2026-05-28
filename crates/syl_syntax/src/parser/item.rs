use super::Parser;
use crate::Attribute;
use std::vec::Vec;
use syl_span::Diagnostic;

impl Parser {
    pub(super) fn parse_attrs_and_doc(
        &mut self,
    ) -> Result<(Vec<Attribute>, Option<String>), Vec<Diagnostic>> {
        let mut doc = self.take_doc_for_next_token();
        let mut attrs = Vec::new();
        while self.check(&crate::lexer::TokenKind::At) {
            let at = self.expect(crate::lexer::TokenKind::At)?.span;
            let name = self.expect_ident()?;
            let name_span = self.prev_span();
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
                attrs.push(Attribute::new(name, args, at.join(name_span)));
            }
            let next_doc = self.take_doc_for_next_token();
            doc = self.merge_doc(doc, next_doc);
        }
        Ok((attrs, doc))
    }

    pub(super) fn parse_path(&mut self) -> Result<Vec<String>, Vec<Diagnostic>> {
        let mut path = vec![self.expect_path_segment()?];
        while self.consume(&crate::lexer::TokenKind::Dot).is_some() {
            path.push(self.expect_path_segment()?);
        }
        Ok(path)
    }

    fn expect_path_segment(&mut self) -> Result<String, Vec<Diagnostic>> {
        match self.peek_kind() {
            Some(crate::lexer::TokenKind::KwBundle) => {
                self.bump();
                Ok("bundle".to_string())
            }
            _ => self.expect_ident(),
        }
    }
}
