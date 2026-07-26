//! Top-level item parsing: `use`, `const`, `fn`, `enum`, `struct`, `bundle`,
//! `interface`, `map`, `cell`, `extern cell`, plus attributes and paths.

use super::{BlockContext, Parser};
use crate::lexer::TokenKind;
use crate::{
    Attribute, BundleItem, CallableItem, ConstItem, DriveCapability, EnumItem, EnumLayout,
    EnumVariant, Expr, ExternCellItem, FnItem, InterfaceItem, Item, MapItem, Param, ParamDirection,
    PortDecl, StructItem, UseItem,
};
use std::vec::Vec;
use syl_span::Diagnostic;

impl Parser {
    pub(super) fn parse_item(&mut self) -> Result<Item, Vec<Diagnostic>> {
        let (attrs, doc) = self.parse_attrs_and_doc()?;
        let mut item = match self.peek_kind() {
            Some(TokenKind::KwUse) => Item::Use(self.parse_use_item()?),
            Some(TokenKind::KwConst) => Item::Const(self.parse_const_item()?),
            Some(TokenKind::KwFn) => Item::Fn(self.parse_fn_item()?),
            Some(TokenKind::KwEnum) => Item::Enum(self.parse_enum_item(attrs)?),
            Some(TokenKind::KwStruct) => Item::Struct(self.parse_struct_item()?),
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
        self.apply_item_doc(&mut item, doc);
        Ok(item)
    }

    pub(super) fn parse_attrs_and_doc(
        &mut self,
    ) -> Result<(Vec<Attribute>, Option<String>), Vec<Diagnostic>> {
        let mut doc = self.take_doc_for_next_token();
        let mut attrs = Vec::new();
        while self.check(&TokenKind::At) {
            let at = self.expect(TokenKind::At)?.span;
            let name = self.expect_ident()?;
            let name_span = self.prev_span();
            let mut args = Vec::new();
            if self.consume(&TokenKind::LParen).is_some() {
                if !self.check(&TokenKind::RParen) {
                    loop {
                        args.push(self.parse_expr(0)?);
                        if self.consume(&TokenKind::Comma).is_none() {
                            break;
                        }
                    }
                }
                let end = self.expect(TokenKind::RParen)?.span;
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
        while self.consume(&TokenKind::Dot).is_some() {
            path.push(self.expect_path_segment()?);
        }
        Ok(path)
    }

    fn expect_path_segment(&mut self) -> Result<String, Vec<Diagnostic>> {
        match self.peek_kind() {
            Some(TokenKind::KwStruct) => {
                self.bump();
                Ok("struct".to_string())
            }
            Some(TokenKind::KwBundle) => {
                self.bump();
                Ok("bundle".to_string())
            }
            _ => self.expect_ident(),
        }
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
            let doc = self.take_doc_for_next_token();
            let vname = self.expect_ident()?;
            let name_span = self.prev_span();
            let value = if self.consume(&TokenKind::Eq).is_some() {
                Some(self.parse_expr(0)?)
            } else {
                None
            };
            let end = value.as_ref().map(Expr::span).unwrap_or(name_span);
            let mut variant = EnumVariant::new(vname, value, name_span.join(end));
            variant.doc = doc;
            variants.push(variant);
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

    fn parse_struct_item(&mut self) -> Result<StructItem, Vec<Diagnostic>> {
        let start = self.expect(TokenKind::KwStruct)?.span;
        let name = self.expect_ident()?;
        let generics = self.parse_generic_params()?;
        let (fields, end) = self.parse_field_block()?;
        Ok(StructItem::builder(name)
            .generics(generics)
            .fields(fields)
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
        let start = self.expect(kw)?.span;
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
            if let Some(port) = ports.last_mut() {
                port.doc = param.doc.clone();
            }
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
}
