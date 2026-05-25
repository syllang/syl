use crate::{
    AstFile, Attribute, Block, BundleItem, CallArg, CallableItem, ConstItem, DriveCapability,
    EnumItem, EnumVariant, ErrorItem, Expr, ExternModuleItem, FieldDecl, FnItem, GenericParam,
    InterfaceItem, MapItem, MatchArm, NamedExpr, Param, ParamDirection, PortDecl, RegReset,
    ResultBinding, SelectArm, TypeExpr, UseItem, ViewDecl, ViewDirection, ViewField,
};
use syl_span::Span;

impl AstFile {
    pub fn new(items: Vec<crate::Item>) -> Self {
        Self { items }
    }
}

impl ErrorItem {
    pub fn new(span: Span) -> Self {
        Self { span }
    }
}

impl UseItem {
    pub fn new(path: Vec<String>, span: Span) -> Self {
        Self { path, span }
    }
}

impl ConstItem {
    pub fn new(name: String, ty: Option<TypeExpr>, value: Expr, span: Span) -> Self {
        Self {
            name,
            ty,
            value,
            span,
        }
    }
}

impl FnItem {
    pub fn builder(name: String, body: Block) -> FnItemBuilder {
        FnItemBuilder {
            name,
            body,
            params: Vec::new(),
            ret_ty: None,
            span: Span::default(),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct FnItemBuilder {
    name: String,
    body: Block,
    params: Vec<Param>,
    ret_ty: Option<TypeExpr>,
    span: Span,
}

impl FnItemBuilder {
    pub fn params(mut self, params: Vec<Param>) -> Self {
        self.params = params;
        self
    }

    pub fn ret_ty(mut self, ret_ty: Option<TypeExpr>) -> Self {
        self.ret_ty = ret_ty;
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn build(self) -> FnItem {
        FnItem {
            name: self.name,
            params: self.params,
            ret_ty: self.ret_ty,
            body: self.body,
            span: self.span,
        }
    }
}

impl EnumItem {
    pub fn new(name: String, variants: Vec<EnumVariant>, span: Span) -> Self {
        Self {
            name,
            variants,
            span,
        }
    }
}

impl EnumVariant {
    pub fn new(name: String, span: Span) -> Self {
        Self { name, span }
    }
}

impl BundleItem {
    pub fn builder(name: String) -> BundleItemBuilder {
        BundleItemBuilder {
            name,
            generics: Vec::new(),
            fields: Vec::new(),
            attrs: Vec::new(),
            span: Span::default(),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct BundleItemBuilder {
    name: String,
    generics: Vec<GenericParam>,
    fields: Vec<FieldDecl>,
    attrs: Vec<Attribute>,
    span: Span,
}

impl BundleItemBuilder {
    pub fn generics(mut self, generics: Vec<GenericParam>) -> Self {
        self.generics = generics;
        self
    }

    pub fn fields(mut self, fields: Vec<FieldDecl>) -> Self {
        self.fields = fields;
        self
    }

    pub fn attrs(mut self, attrs: Vec<Attribute>) -> Self {
        self.attrs = attrs;
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn build(self) -> BundleItem {
        BundleItem {
            name: self.name,
            generics: self.generics,
            fields: self.fields,
            attrs: self.attrs,
            span: self.span,
        }
    }
}

impl InterfaceItem {
    pub fn builder(name: String) -> InterfaceItemBuilder {
        InterfaceItemBuilder {
            name,
            generics: Vec::new(),
            fields: Vec::new(),
            views: Vec::new(),
            span: Span::default(),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct InterfaceItemBuilder {
    name: String,
    generics: Vec<GenericParam>,
    fields: Vec<FieldDecl>,
    views: Vec<ViewDecl>,
    span: Span,
}

impl InterfaceItemBuilder {
    pub fn generics(mut self, generics: Vec<GenericParam>) -> Self {
        self.generics = generics;
        self
    }

    pub fn fields(mut self, fields: Vec<FieldDecl>) -> Self {
        self.fields = fields;
        self
    }

    pub fn views(mut self, views: Vec<ViewDecl>) -> Self {
        self.views = views;
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn build(self) -> InterfaceItem {
        InterfaceItem {
            name: self.name,
            generics: self.generics,
            fields: self.fields,
            views: self.views,
            span: self.span,
        }
    }
}

impl MapItem {
    pub fn builder(name: String, body: Expr) -> MapItemBuilder {
        MapItemBuilder {
            name,
            body,
            generics: Vec::new(),
            params: Vec::new(),
            ret_ty: None,
            span: Span::default(),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct MapItemBuilder {
    name: String,
    body: Expr,
    generics: Vec<GenericParam>,
    params: Vec<Param>,
    ret_ty: Option<TypeExpr>,
    span: Span,
}

impl MapItemBuilder {
    pub fn generics(mut self, generics: Vec<GenericParam>) -> Self {
        self.generics = generics;
        self
    }

    pub fn params(mut self, params: Vec<Param>) -> Self {
        self.params = params;
        self
    }

    pub fn ret_ty(mut self, ret_ty: Option<TypeExpr>) -> Self {
        self.ret_ty = ret_ty;
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn build(self) -> MapItem {
        MapItem {
            name: self.name,
            generics: self.generics,
            params: self.params,
            ret_ty: self.ret_ty,
            body: self.body,
            span: self.span,
        }
    }
}

impl CallableItem {
    pub fn builder(name: String, body: Block) -> CallableItemBuilder {
        CallableItemBuilder {
            name,
            body,
            generics: Vec::new(),
            params: Vec::new(),
            ports: Vec::new(),
            result: None,
            span: Span::default(),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct CallableItemBuilder {
    name: String,
    body: Block,
    generics: Vec<GenericParam>,
    params: Vec<Param>,
    ports: Vec<PortDecl>,
    result: Option<ResultBinding>,
    span: Span,
}

impl CallableItemBuilder {
    pub fn generics(mut self, generics: Vec<GenericParam>) -> Self {
        self.generics = generics;
        self
    }

    pub fn params(mut self, params: Vec<Param>) -> Self {
        self.params = params;
        self
    }

    pub fn ports(mut self, ports: Vec<PortDecl>) -> Self {
        self.ports = ports;
        self
    }

    pub fn result(mut self, result: Option<ResultBinding>) -> Self {
        self.result = result;
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn build(self) -> CallableItem {
        CallableItem {
            name: self.name,
            generics: self.generics,
            params: self.params,
            ports: self.ports,
            result: self.result,
            body: self.body,
            span: self.span,
        }
    }
}

impl ExternModuleItem {
    pub fn builder(name: String) -> ExternModuleItemBuilder {
        ExternModuleItemBuilder {
            name,
            generics: Vec::new(),
            params: Vec::new(),
            ports: Vec::new(),
            result: None,
            span: Span::default(),
        }
    }
}

#[derive(Debug)]
#[non_exhaustive]
pub struct ExternModuleItemBuilder {
    name: String,
    generics: Vec<GenericParam>,
    params: Vec<Param>,
    ports: Vec<PortDecl>,
    result: Option<ResultBinding>,
    span: Span,
}

impl ExternModuleItemBuilder {
    pub fn generics(mut self, generics: Vec<GenericParam>) -> Self {
        self.generics = generics;
        self
    }

    pub fn params(mut self, params: Vec<Param>) -> Self {
        self.params = params;
        self
    }

    pub fn ports(mut self, ports: Vec<PortDecl>) -> Self {
        self.ports = ports;
        self
    }

    pub fn result(mut self, result: Option<ResultBinding>) -> Self {
        self.result = result;
        self
    }

    pub fn span(mut self, span: Span) -> Self {
        self.span = span;
        self
    }

    pub fn build(self) -> ExternModuleItem {
        ExternModuleItem {
            name: self.name,
            generics: self.generics,
            params: self.params,
            ports: self.ports,
            result: self.result,
            span: self.span,
        }
    }
}

impl ResultBinding {
    pub fn new(name: String, ty: TypeExpr, drive: DriveCapability, span: Span) -> Self {
        Self {
            name,
            ty,
            drive,
            span,
        }
    }
}

impl PortDecl {
    pub fn new(
        name: String,
        dir: ParamDirection,
        ty: TypeExpr,
        drive: DriveCapability,
        span: Span,
    ) -> Self {
        Self {
            name,
            dir,
            ty,
            drive,
            span,
        }
    }
}

impl Param {
    pub fn new(name: String, dir: Option<ParamDirection>, ty: TypeExpr, span: Span) -> Self {
        Self {
            name,
            dir,
            ty,
            span,
        }
    }
}

impl GenericParam {
    pub fn new(name: String, kind: Option<TypeExpr>, default: Option<Expr>, span: Span) -> Self {
        Self {
            name,
            kind,
            default,
            span,
        }
    }
}

impl FieldDecl {
    pub fn new(name: String, ty: TypeExpr, span: Span) -> Self {
        Self { name, ty, span }
    }
}

impl Attribute {
    pub fn new(name: String, args: Vec<Expr>, span: Span) -> Self {
        Self { name, args, span }
    }
}

impl ViewDecl {
    pub fn new(name: String, fields: Vec<ViewField>, span: Span) -> Self {
        Self { name, fields, span }
    }
}

impl ViewField {
    pub fn new(dir: ViewDirection, name: String, span: Span) -> Self {
        Self { dir, name, span }
    }
}

impl Block {
    pub fn new(stmts: Vec<crate::Stmt>, tail: Option<Box<Expr>>, span: Span) -> Self {
        Self { stmts, tail, span }
    }
}

impl RegReset {
    pub fn new(domain: Option<Expr>, value: Expr, span: Span) -> Self {
        Self {
            domain,
            value,
            span,
        }
    }
}

impl NamedExpr {
    pub fn new(name: String, value: Expr, span: Span) -> Self {
        Self { name, value, span }
    }
}

impl CallArg {
    pub fn new(name: Option<String>, value: Expr, span: Span) -> Self {
        Self { name, value, span }
    }
}

impl SelectArm {
    pub fn new(pattern: Expr, value: Expr, span: Span) -> Self {
        Self {
            pattern,
            value,
            span,
        }
    }
}

impl MatchArm {
    pub fn new(pattern: crate::Pattern, value: Expr, span: Span) -> Self {
        Self {
            pattern,
            value,
            span,
        }
    }
}
