use crate::ast::{
    BundleItemBuilder, CallableItemBuilder, ExternCellItemBuilder, FnItemBuilder,
    InterfaceItemBuilder, MapItemBuilder,
};
use crate::{
    AstFile, Attribute, Block, BundleItem, CallArg, CallableItem, ConstItem, EnumItem, EnumVariant,
    ErrorItem, Expr, ExternCellItem, FieldDecl, FnItem, GenericParam, InterfaceItem, MapItem,
    MatchArm, NamedExpr, Param, ParamDirection, ParamRole, PortDecl, RegReset, ResultBinding,
    SelectArm, SelectMode, Stmt, TypeExpr, UseItem, ViewDecl, ViewDirection, ViewField,
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
        FnItemBuilder::default().name(name).body(body)
    }
}

impl FnItemBuilder {
    pub fn build(self) -> FnItem {
        self.try_build()
            .expect("FnItemBuilder must be initialized with name and body")
    }
}

impl BundleItem {
    pub fn builder(name: String) -> BundleItemBuilder {
        BundleItemBuilder::default().name(name)
    }
}

impl BundleItemBuilder {
    pub fn build(self) -> BundleItem {
        self.try_build()
            .expect("BundleItemBuilder must be initialized with name")
    }
}

impl InterfaceItem {
    pub fn builder(name: String) -> InterfaceItemBuilder {
        InterfaceItemBuilder::default().name(name)
    }
}

impl InterfaceItemBuilder {
    pub fn build(self) -> InterfaceItem {
        self.try_build()
            .expect("InterfaceItemBuilder must be initialized with name")
    }
}

impl MapItem {
    pub fn builder(name: String, body: Expr) -> MapItemBuilder {
        MapItemBuilder::default().name(name).body(body)
    }
}

impl MapItemBuilder {
    pub fn build(self) -> MapItem {
        self.try_build()
            .expect("MapItemBuilder must be initialized with name and body")
    }
}

impl CallableItem {
    pub fn builder(name: String, body: Block) -> CallableItemBuilder {
        CallableItemBuilder::default().name(name).body(body)
    }
}

impl CallableItemBuilder {
    pub fn build(self) -> CallableItem {
        self.try_build()
            .expect("CallableItemBuilder must be initialized with name and body")
    }
}

impl ExternCellItem {
    pub fn builder(name: String) -> ExternCellItemBuilder {
        ExternCellItemBuilder::default().name(name)
    }
}

impl ExternCellItemBuilder {
    pub fn build(self) -> ExternCellItem {
        self.try_build()
            .expect("ExternModuleItemBuilder must be initialized with name")
    }
}

impl ResultBinding {
    pub fn new(name: String, ty: TypeExpr, drive: crate::DriveCapability, span: Span) -> Self {
        Self {
            name,
            ty,
            drive,
            span,
        }
    }

    pub fn is_drivable(&self) -> bool {
        self.drive.can_write()
    }
}

impl PortDecl {
    pub fn new(
        name: String,
        dir: ParamDirection,
        ty: TypeExpr,
        drive: crate::DriveCapability,
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

    pub fn is_in(&self) -> bool {
        self.dir.is_in()
    }

    pub fn is_out(&self) -> bool {
        self.dir.is_out()
    }
}

impl Param {
    pub fn new(name: String, dir: Option<ParamDirection>, ty: TypeExpr, span: Span) -> Self {
        Self {
            name,
            dir,
            ty,
            role: ParamRole::Ordinary,
            span,
        }
    }

    pub fn receiver(name: String, ty: TypeExpr, span: Span) -> Self {
        Self {
            name,
            dir: None,
            ty,
            role: ParamRole::Receiver,
            span,
        }
    }

    pub fn is_receiver(&self) -> bool {
        matches!(self.role, ParamRole::Receiver)
    }
}

impl ParamDirection {
    pub fn is_in(&self) -> bool {
        matches!(self, Self::In | Self::InOut)
    }

    pub fn is_out(&self) -> bool {
        matches!(self, Self::InOut | Self::Out)
    }
}

impl ViewDirection {
    pub fn is_in(&self) -> bool {
        matches!(self, Self::In | Self::InOut)
    }

    pub fn is_out(&self) -> bool {
        matches!(self, Self::InOut | Self::Out)
    }
}

impl SelectMode {
    pub fn is_unique(&self) -> bool {
        matches!(self, Self::Unique)
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
    pub fn new(stmts: Vec<Stmt>, tail: Option<Box<Expr>>, span: Span) -> Self {
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

impl Attribute {
    pub fn new(name: String, args: Vec<Expr>, span: Span) -> Self {
        Self { name, args, span }
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
