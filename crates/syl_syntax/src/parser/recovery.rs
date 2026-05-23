use super::Parser;
use crate::lexer::TokenKind;
use syl_span::Span;

impl Parser {
    pub(super) fn recover_item_boundary(&mut self, start_pos: usize) -> Span {
        let start = self
            .tokens
            .get(start_pos)
            .map(|token| token.span)
            .unwrap_or_else(|| self.prev_span());
        if self.pos <= start_pos {
            self.bump();
        }
        let mut end = self.prev_span();
        let mut brace_depth = 0usize;
        while !self.is_eof() {
            if brace_depth == 0
                && self.pos > start_pos
                && self
                    .peek_kind()
                    .is_some_and(|kind| self.is_item_start(kind))
            {
                return start.join(end);
            }
            let Some(token) = self.bump() else {
                return start.join(end);
            };
            end = token.span;
            match token.kind {
                TokenKind::LBrace => brace_depth = brace_depth.saturating_add(1),
                TokenKind::RBrace => {
                    brace_depth = brace_depth.saturating_sub(1);
                    if brace_depth == 0 {
                        return start.join(end);
                    }
                }
                TokenKind::Semi if brace_depth == 0 => return start.join(end),
                _ => {}
            }
        }
        start.join(end)
    }

    pub(super) fn recover_stmt_boundary(&mut self, start_pos: usize) -> Span {
        if self.pos.saturating_sub(1) > start_pos
            && self
                .tokens
                .get(self.pos.saturating_sub(1))
                .is_some_and(|token| {
                    token.kind == TokenKind::RBrace || self.is_stmt_start(&token.kind)
                })
        {
            self.pos = self.pos.saturating_sub(1);
            let start = self
                .tokens
                .get(start_pos)
                .map(|token| token.span)
                .unwrap_or_else(|| self.prev_span());
            return start.join(self.prev_span());
        }

        if self.pos <= start_pos {
            self.bump();
        }

        let start = self
            .tokens
            .get(start_pos)
            .map(|token| token.span)
            .unwrap_or_else(|| self.prev_span());
        let mut end = self.prev_span();
        let mut brace_depth = 0usize;
        let mut paren_depth = 0usize;
        let mut bracket_depth = 0usize;

        while !self.is_eof() {
            let at_stmt_boundary = brace_depth == 0 && paren_depth == 0 && bracket_depth == 0;
            if at_stmt_boundary {
                if self.check(&TokenKind::RBrace) {
                    return start.join(end);
                }
                if self.pos > start_pos
                    && self
                        .peek_kind()
                        .is_some_and(|kind| self.is_stmt_start(kind))
                {
                    return start.join(end);
                }
            }

            let Some(token) = self.bump() else {
                return start.join(end);
            };
            end = token.span;
            match token.kind {
                TokenKind::LBrace => brace_depth = brace_depth.saturating_add(1),
                TokenKind::RBrace => {
                    if brace_depth == 0 {
                        return start.join(end);
                    }
                    brace_depth = brace_depth.saturating_sub(1);
                }
                TokenKind::LParen => paren_depth = paren_depth.saturating_add(1),
                TokenKind::RParen => paren_depth = paren_depth.saturating_sub(1),
                TokenKind::LBracket => bracket_depth = bracket_depth.saturating_add(1),
                TokenKind::RBracket => bracket_depth = bracket_depth.saturating_sub(1),
                TokenKind::Semi if at_stmt_boundary => return start.join(end),
                _ => {}
            }
        }

        start.join(end)
    }

    fn is_item_start(&self, kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::KwPackage
                | TokenKind::KwUse
                | TokenKind::KwConst
                | TokenKind::KwFn
                | TokenKind::KwEnum
                | TokenKind::KwBundle
                | TokenKind::KwInterface
                | TokenKind::KwMap
                | TokenKind::KwCell
                | TokenKind::KwModule
                | TokenKind::KwExtern
        )
    }

    fn is_stmt_start(&self, kind: &TokenKind) -> bool {
        matches!(
            kind,
            TokenKind::KwLet
                | TokenKind::KwConst
                | TokenKind::KwAlias
                | TokenKind::KwVar
                | TokenKind::KwSignal
                | TokenKind::KwReg
                | TokenKind::KwNext
                | TokenKind::KwInst
                | TokenKind::KwWhile
                | TokenKind::KwFor
                | TokenKind::KwIf
                | TokenKind::KwReturn
        )
    }
}
