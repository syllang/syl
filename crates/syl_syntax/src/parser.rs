use crate::lexer::{LexemeKind, Lexer, LosslessLexer, Token, TokenKind};
use crate::*;
use syl_span::{Diagnostic, SourceId, Span};

mod expr;
mod item;
mod lossless_tree;
mod output;
mod recovery;
mod span_ext;
mod stmt;

pub use output::ParseOutput;

#[derive(Debug)]
enum BlockEntry {
    Stmt(Box<Stmt>),
    Tail(Expr),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BlockContext {
    Function,
    Hardware,
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct SourceParser<'a> {
    source: &'a str,
    source_id: SourceId,
}

impl<'a> SourceParser<'a> {
    pub fn new(source: &'a str) -> Self {
        Self::new_in(source, SourceId::default())
    }

    pub fn new_in(source: &'a str, source_id: SourceId) -> Self {
        Self { source, source_id }
    }

    pub fn parse_file(&self) -> Result<AstFile, Vec<Diagnostic>> {
        self.parse_file_partial().into_result()
    }

    pub fn parse_file_partial(&self) -> ParseOutput {
        let mut lexer = Lexer::new_in(self.source, self.source_id);
        let output = lexer.lex_all_partial();
        let mut parsed = Parser::new_at_end(output.tokens, self.source_id, self.source.len())
            .parse_file_partial();
        parsed.diagnostics.extend(output.diagnostics);
        parsed.attach_node_index(self.source);
        parsed
    }

    pub fn parse_file_with_lossless(&self) -> (ParseOutput, LosslessSyntaxFile) {
        let mut lexer = LosslessLexer::new_in(self.source, self.source_id);
        let output = lexer.lex_all_partial();
        let mut parse_tokens = Vec::new();
        let mut syntax_tokens = Vec::new();

        for lexeme in output.lexemes {
            let syntax_kind = match &lexeme.kind {
                LexemeKind::Token(kind) => match kind {
                    TokenKind::Ident(_) => LosslessTokenKind::Ident,
                    TokenKind::Int(_) => LosslessTokenKind::Int,
                    TokenKind::Str(_) => LosslessTokenKind::Str,
                    TokenKind::Bool(_) => LosslessTokenKind::Bool,
                    TokenKind::KwUse
                    | TokenKind::KwConst
                    | TokenKind::KwFn
                    | TokenKind::KwLet
                    | TokenKind::KwReturn
                    | TokenKind::KwThis
                    | TokenKind::KwVar
                    | TokenKind::KwFor
                    | TokenKind::KwWhile
                    | TokenKind::KwIf
                    | TokenKind::KwElse
                    | TokenKind::KwMatch
                    | TokenKind::KwSelect
                    | TokenKind::KwPriority
                    | TokenKind::KwUnique
                    | TokenKind::KwEnum
                    | TokenKind::KwBundle
                    | TokenKind::KwInterface
                    | TokenKind::KwView
                    | TokenKind::KwMap
                    | TokenKind::KwCell
                    | TokenKind::KwExtern
                    | TokenKind::KwSignal
                    | TokenKind::KwReg
                    | TokenKind::KwPlace
                    | TokenKind::KwInplace
                    | TokenKind::KwNext
                    | TokenKind::KwIn
                    | TokenKind::KwInOut
                    | TokenKind::KwOut
                    | TokenKind::KwAnd
                    | TokenKind::KwOr
                    | TokenKind::KwNot
                    | TokenKind::KwXor
                    | TokenKind::KwEqWord => LosslessTokenKind::Keyword,
                    TokenKind::At
                    | TokenKind::Plus
                    | TokenKind::Minus
                    | TokenKind::Star
                    | TokenKind::Slash
                    | TokenKind::Percent
                    | TokenKind::Eq
                    | TokenKind::EqEq
                    | TokenKind::Bang
                    | TokenKind::BangEq
                    | TokenKind::Lt
                    | TokenKind::LtEq
                    | TokenKind::LtLt
                    | TokenKind::Gt
                    | TokenKind::GtEq
                    | TokenKind::AndAnd
                    | TokenKind::OrOr
                    | TokenKind::Dot
                    | TokenKind::DotDot
                    | TokenKind::Comma
                    | TokenKind::Colon
                    | TokenKind::ColonEq
                    | TokenKind::Semi
                    | TokenKind::Arrow
                    | TokenKind::EqGt
                    | TokenKind::LParen
                    | TokenKind::RParen
                    | TokenKind::LBrace
                    | TokenKind::RBrace
                    | TokenKind::LBracket
                    | TokenKind::RBracket => LosslessTokenKind::Punctuation,
                },
                LexemeKind::Whitespace => LosslessTokenKind::Whitespace,
                LexemeKind::LineComment => LosslessTokenKind::LineComment,
                LexemeKind::Unknown => LosslessTokenKind::Unknown,
            };
            if let LexemeKind::Token(kind) = lexeme.kind {
                parse_tokens.push(Token::new(kind, lexeme.span));
            }
            syntax_tokens.push(LosslessToken::new(syntax_kind, lexeme.span, lexeme.text));
        }

        let mut parsed = Parser::new_at_end(parse_tokens, self.source_id, self.source.len())
            .parse_file_partial();
        parsed.diagnostics.extend(output.diagnostics);
        parsed.attach_node_index(self.source);
        let syntax = lossless_tree::build_lossless_syntax_file(
            self.source_id,
            self.source.len(),
            &parsed.file,
            syntax_tokens,
        );
        (parsed, syntax)
    }

    pub fn parse_expr(&self) -> Result<Expr, Vec<Diagnostic>> {
        let tokens = Lexer::new_in(self.source, self.source_id).lex_all()?;
        Parser::new_at_end(tokens, self.source_id, self.source.len()).parse_expr(0)
    }
}

#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    eof_span: Span,
    block_context: BlockContext,
    diagnostics: Vec<Diagnostic>,
}

impl Parser {
    #[cfg(test)]
    fn new(tokens: Vec<Token>) -> Self {
        let eof_span = tokens
            .last()
            .map(|token| Span::new_in(token.span.source, token.span.end, token.span.end))
            .unwrap_or_default();
        Self::new_with_eof(tokens, eof_span)
    }

    fn new_at_end(tokens: Vec<Token>, source_id: SourceId, source_len: usize) -> Self {
        Self::new_with_eof(tokens, Span::new_in(source_id, source_len, source_len))
    }

    fn new_with_eof(tokens: Vec<Token>, eof_span: Span) -> Self {
        Self {
            tokens,
            pos: 0,
            eof_span,
            block_context: BlockContext::Function,
            diagnostics: Vec::new(),
        }
    }

    pub fn parse_file(self) -> Result<AstFile, Vec<Diagnostic>> {
        self.parse_file_partial().into_result()
    }

    pub fn parse_file_partial(mut self) -> ParseOutput {
        let mut items = Vec::new();
        while !self.is_eof() {
            let start_pos = self.pos;
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(mut diagnostics) => {
                    self.diagnostics.append(&mut diagnostics);
                    let span = self.recover_item_boundary(start_pos);
                    items.push(Item::Error(ErrorItem::new(span)));
                }
            }
        }
        ParseOutput::new(AstFile::new(items), self.diagnostics)
    }

    fn parse_item(&mut self) -> Result<Item, Vec<Diagnostic>> {
        let attrs = self.parse_attrs()?;
        let item = match self.peek_kind() {
            Some(TokenKind::KwUse) => Item::Use(self.parse_use_item()?),
            Some(TokenKind::KwConst) => Item::Const(self.parse_const_item()?),
            Some(TokenKind::KwFn) => Item::Fn(self.parse_fn_item()?),
            Some(TokenKind::KwEnum) => Item::Enum(self.parse_enum_item(attrs)?),
            Some(TokenKind::KwBundle) => Item::Bundle(self.parse_bundle_item(attrs)?),
            Some(TokenKind::KwInterface) => Item::Interface(self.parse_interface_item()?),
            Some(TokenKind::KwMap) => Item::Map(self.parse_map_item()?),
            Some(TokenKind::KwCell) => Item::Cell(self.parse_callable_item(TokenKind::KwCell)?),
            Some(TokenKind::KwExtern) => {
                self.expect(TokenKind::KwExtern)?;
                self.expect(TokenKind::KwCell)?;
                Item::ExternCell(self.parse_extern_cell_item()?)
            }
            Some(_) => {
                let span = self.peek().map(|t| t.span).unwrap_or_default();
                self.error(span, "expected item");
                self.bump();
                return Err(std::mem::take(&mut self.diagnostics));
            }
            None => return Err(std::mem::take(&mut self.diagnostics)),
        };
        Ok(item)
    }

    fn parse_use_item(&mut self) -> Result<UseItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwUse)?.span;
        let path = self.parse_path()?;
        let end = self
            .consume(&TokenKind::Semi)
            .map(|tok| tok.span)
            .unwrap_or_else(|| self.prev_span());
        Ok(UseItem::new(path, start.join(end)))
    }

    fn parse_const_item(&mut self) -> Result<ConstItem, Vec<Diagnostic>> {
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
        Ok(ConstItem::new(name, ty, value, start.join(end)))
    }

    fn parse_enum_item(&mut self, attrs: Vec<Attribute>) -> Result<EnumItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwEnum)?.span;
        let name = self.expect_ident()?;
        let width = if self.consume(&TokenKind::Colon).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let layout = self.enum_layout_from_attrs(&attrs)?;
        if self.peek_kind() == Some(&TokenKind::LBrace) {
            self.expect(TokenKind::LBrace)?;
        }
        let mut variants = Vec::new();
        while !self.check(&TokenKind::RBrace) && !self.is_eof() {
            let vname = self.expect_ident()?;
            let name_span = self.prev_span();
            let value = if self.consume(&TokenKind::Eq).is_some() {
                Some(self.parse_expr(0)?)
            } else {
                None
            };
            let end = value.as_ref().map(Expr::span).unwrap_or(name_span);
            variants.push(EnumVariant::new(vname, value, name_span.join(end)));
            self.consume(&TokenKind::Comma);
        }
        let end = self.expect(TokenKind::RBrace)?.span;
        Ok(EnumItem::new(
            name,
            width,
            layout,
            variants,
            start.join(end),
        ))
    }

    fn parse_bundle_item(&mut self, attrs: Vec<Attribute>) -> Result<BundleItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwBundle)?.span;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let (fields, end) = self.parse_field_block()?;
        Ok(BundleItem::builder(name)
            .generics(generics)
            .fields(fields)
            .attrs(attrs)
            .span(start.join(end))
            .build())
    }

    fn parse_interface_item(&mut self) -> Result<InterfaceItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwInterface)?.span;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let (fields, views, end) = self.parse_interface_body()?;
        Ok(InterfaceItem::builder(name)
            .generics(generics)
            .fields(fields)
            .views(views)
            .span(start.join(end))
            .build())
    }

    fn parse_map_item(&mut self) -> Result<MapItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwMap)?.span;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let params = self.parse_param_list()?;
        let ret_ty = if self.consume(&TokenKind::Arrow).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        self.expect(TokenKind::Eq)?;
        let body = self.parse_expr(0)?;
        let end = body.span();
        Ok(MapItem::builder(name, body)
            .generics(generics)
            .params(params)
            .ret_ty(ret_ty)
            .span(start.join(end))
            .build())
    }

    fn parse_callable_item(&mut self, kw: TokenKind) -> Result<CallableItem, Vec<Diagnostic>> {
        let start = self.expect(kw.clone())?.span;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let params = self.parse_param_list()?;
        let ports = self.parse_ports_from_params(&params)?;
        let result = if self.consume(&TokenKind::Arrow).is_some() {
            Some(self.parse_result_binding()?)
        } else {
            None
        };
        let body = self.parse_block(BlockContext::Hardware)?;
        let span = start.join(body.span);
        Ok(CallableItem::builder(name, body)
            .generics(generics)
            .params(params)
            .ports(ports)
            .result(result)
            .span(span)
            .build())
    }

    fn parse_extern_cell_item(&mut self) -> Result<ExternCellItem, Vec<Diagnostic>> {
        let start = self.prev_span();
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let params = self.parse_param_list()?;
        let ports = self.parse_ports_from_params(&params)?;
        let result = if self.consume(&TokenKind::Arrow).is_some() {
            Some(self.parse_result_binding()?)
        } else {
            None
        };
        let end = result
            .as_ref()
            .map(|result| result.span)
            .unwrap_or_else(|| self.prev_span());
        Ok(ExternCellItem::builder(name)
            .generics(generics)
            .params(params)
            .ports(ports)
            .result(result)
            .span(start.join(end))
            .build())
    }

    fn parse_ports_from_params(
        &mut self,
        params: &[Param],
    ) -> Result<Vec<PortDecl>, Vec<Diagnostic>> {
        let mut ports = Vec::new();
        for param in params {
            if param.is_receiver() {
                self.error(param.span, "cell ports cannot use `this` receiver");
                return Err(std::mem::take(&mut self.diagnostics));
            }
            let Some(dir) = param.dir else {
                self.error(
                    param.span,
                    "module and cell ports require explicit in/out direction",
                );
                return Err(std::mem::take(&mut self.diagnostics));
            };
            let drive = match dir {
                ParamDirection::In => DriveCapability::ReadOnly,
                ParamDirection::InOut => DriveCapability::ReadWrite,
                ParamDirection::Out => DriveCapability::WriteOnly,
            };
            ports.push(PortDecl::new(
                param.name.clone(),
                dir,
                param.ty.clone(),
                drive,
                param.span,
            ));
        }
        Ok(ports)
    }

    fn parse_fn_item(&mut self) -> Result<FnItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwFn)?.span;
        let name = self.expect_ident()?;
        let params = self.parse_param_list()?;
        let ret_ty = if self.consume(&TokenKind::Arrow).is_some() {
            Some(self.parse_type_expr()?)
        } else {
            None
        };
        let body = self.parse_block(BlockContext::Function)?;
        let span = start.join(body.span);
        Ok(FnItem::builder(name, body)
            .params(params)
            .ret_ty(ret_ty)
            .span(span)
            .build())
    }

    fn parse_block(&mut self, context: BlockContext) -> Result<Block, Vec<Diagnostic>> {
        let previous_context = self.block_context;
        self.block_context = context;
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

    fn expect_ident(&mut self) -> Result<String, Vec<Diagnostic>> {
        match self.bump() {
            Some(Token {
                kind: TokenKind::Ident(name),
                ..
            }) => Ok(name),
            Some(tok) => {
                self.error(tok.span, "expected identifier");
                Err(std::mem::take(&mut self.diagnostics))
            }
            None => {
                self.error(self.eof_span(), "unexpected end of source");
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }

    fn enum_layout_from_attrs(
        &mut self,
        attrs: &[Attribute],
    ) -> Result<EnumLayout, Vec<Diagnostic>> {
        let mut layout = EnumLayout::Ordinal;
        let mut seen_layout = false;
        for attr in attrs {
            if attr.name != "layout" {
                self.error(
                    attr.span,
                    format!("unknown enum attribute `@{}`", attr.name),
                );
                return Err(std::mem::take(&mut self.diagnostics));
            }
            if seen_layout {
                self.error(attr.span, "duplicate enum layout attribute");
                return Err(std::mem::take(&mut self.diagnostics));
            }
            seen_layout = true;
            layout = self.parse_enum_layout_attr(attr)?;
        }
        Ok(layout)
    }

    fn parse_enum_layout_attr(&mut self, attr: &Attribute) -> Result<EnumLayout, Vec<Diagnostic>> {
        let [arg] = attr.args.as_slice() else {
            self.error(attr.span, "expected `@layout(name)`");
            return Err(std::mem::take(&mut self.diagnostics));
        };
        let Expr::Ident(name, _) = arg else {
            self.error(arg.span(), "enum layout must be an identifier");
            return Err(std::mem::take(&mut self.diagnostics));
        };
        match name.as_str() {
            "ordinal" => Ok(EnumLayout::Ordinal),
            "flags" => Ok(EnumLayout::Flags),
            "onehot" => Ok(EnumLayout::OneHot),
            other => {
                self.error(arg.span(), format!("unknown enum layout `{other}`"));
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }

    fn expect(&mut self, kind: TokenKind) -> Result<Token, Vec<Diagnostic>> {
        match self.bump() {
            Some(tok) if tok.kind == kind => Ok(tok),
            Some(tok) => {
                self.error(tok.span, format!("expected {:?}", kind));
                Err(std::mem::take(&mut self.diagnostics))
            }
            None => {
                self.error(self.eof_span(), format!("expected {:?}", kind));
                Err(std::mem::take(&mut self.diagnostics))
            }
        }
    }

    fn consume(&mut self, kind: &TokenKind) -> Option<Token> {
        if self.check(kind) { self.bump() } else { None }
    }

    fn check(&self, kind: &TokenKind) -> bool {
        self.peek_kind() == Some(kind)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos)
    }
    fn peek_kind(&self) -> Option<&TokenKind> {
        self.peek().map(|t| &t.kind)
    }
    fn bump(&mut self) -> Option<Token> {
        let tok = self.tokens.get(self.pos).cloned();
        if tok.is_some() {
            self.pos += 1;
        }
        tok
    }
    fn is_eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn eof_span(&self) -> Span {
        self.eof_span
    }

    fn block_context(&self) -> BlockContext {
        self.block_context
    }

    fn prev_span(&self) -> Span {
        self.tokens
            .get(self.pos.saturating_sub(1))
            .map(|t| t.span)
            .unwrap_or_default()
    }
    fn error(&mut self, span: Span, message: impl Into<String>) {
        self.diagnostics.push(
            Diagnostic::new(span, message)
                .with_code("E_SYNTAX_PARSE")
                .with_source("syl_syntax::parser"),
        );
    }
}

#[cfg(test)]
mod tests;
