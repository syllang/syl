use super::{HirBlock, HirBodyExpr, MirTypeRef};
use crate::LocalId;
use syl_span::Span;
use syl_syntax::{
    BundleItem, ConstItem, DriveCapability, EnumItem, ExternModuleItem, FieldDecl, FnItem,
    GenericParam, InterfaceItem, MapItem, Param, ParamDirection, PortDecl, TypeExpr, ViewDirection,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirPortDirection {
    In,
    InOut,
    Out,
}

impl From<&Param> for HirPortDirection {
    fn from(param: &Param) -> Self {
        Self::from(param.dir.as_ref())
    }
}

impl From<&PortDecl> for HirPortDirection {
    fn from(port: &PortDecl) -> Self {
        Self::from(&port.dir)
    }
}

impl From<&HirSignatureParam> for HirPortDirection {
    fn from(param: &HirSignatureParam) -> Self {
        param.direction
    }
}

impl From<&ParamDirection> for HirPortDirection {
    fn from(direction: &ParamDirection) -> Self {
        match direction {
            ParamDirection::InOut => Self::InOut,
            ParamDirection::Out => Self::Out,
            ParamDirection::In => Self::In,
            _ => Self::In,
        }
    }
}

impl From<Option<&ParamDirection>> for HirPortDirection {
    fn from(direction: Option<&ParamDirection>) -> Self {
        match direction {
            Some(direction) => Self::from(direction),
            None => Self::In,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirDriveCapability {
    ReadOnly,
    ReadWrite,
    WriteOnly,
}

impl From<&DriveCapability> for HirDriveCapability {
    fn from(value: &DriveCapability) -> Self {
        match value {
            DriveCapability::ReadOnly => Self::ReadOnly,
            DriveCapability::ReadWrite => Self::ReadWrite,
            DriveCapability::WriteOnly => Self::WriteOnly,
            _ => Self::ReadOnly,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirSignatureGenericParam {
    pub id: Option<LocalId>,
    pub name: String,
    pub kind: Option<MirTypeRef>,
    pub default: Option<HirBodyExpr>,
    pub span: Span,
}

impl From<&GenericParam> for HirSignatureGenericParam {
    fn from(param: &GenericParam) -> Self {
        Self {
            id: None,
            name: param.name.clone(),
            kind: param.kind.as_ref().map(MirTypeRef::from),
            default: param.default.as_ref().map(HirBodyExpr::from_syntax),
            span: param.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirSignatureParam {
    pub id: Option<LocalId>,
    pub name: String,
    pub direction: HirPortDirection,
    pub ty: MirTypeRef,
    pub receiver: bool,
    pub span: Span,
}

impl From<&Param> for HirSignatureParam {
    fn from(param: &Param) -> Self {
        Self {
            id: None,
            name: param.name.clone(),
            direction: HirPortDirection::from(param.dir.as_ref()),
            ty: MirTypeRef::from(&param.ty),
            receiver: param.receiver,
            span: param.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirSignatureResultBinding {
    pub id: Option<LocalId>,
    pub name: String,
    pub ty: MirTypeRef,
    pub drive: HirDriveCapability,
    pub span: Span,
}

impl From<&syl_syntax::ResultBinding> for HirSignatureResultBinding {
    fn from(result: &syl_syntax::ResultBinding) -> Self {
        Self {
            id: None,
            name: result.name.clone(),
            ty: MirTypeRef::from(&result.ty),
            drive: HirDriveCapability::from(&result.drive),
            span: result.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirReturnType {
    pub ty: MirTypeRef,
}

impl From<&TypeExpr> for HirReturnType {
    fn from(ty: &TypeExpr) -> Self {
        Self {
            ty: MirTypeRef::from(ty),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirViewDirection {
    In,
    InOut,
    Out,
}

impl From<&ViewDirection> for HirViewDirection {
    fn from(direction: &ViewDirection) -> Self {
        match direction {
            ViewDirection::InOut => Self::InOut,
            ViewDirection::Out => Self::Out,
            ViewDirection::In => Self::In,
            _ => Self::In,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirEnumVariantDecl {
    pub name: String,
    pub span: Span,
}

impl From<&syl_syntax::EnumVariant> for HirEnumVariantDecl {
    fn from(variant: &syl_syntax::EnumVariant) -> Self {
        Self {
            name: variant.name.clone(),
            span: variant.span,
        }
    }
}

impl HirEnumVariantDecl {
    fn summary_count(&self) -> usize {
        self.name.len() + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirFieldDecl {
    pub name: String,
    pub ty: MirTypeRef,
    pub span: Span,
}

impl From<&FieldDecl> for HirFieldDecl {
    fn from(field: &FieldDecl) -> Self {
        Self {
            name: field.name.clone(),
            ty: MirTypeRef::from(&field.ty),
            span: field.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirAttribute {
    pub name: String,
    pub args: Vec<HirBodyExpr>,
    pub span: Span,
}

impl From<&syl_syntax::Attribute> for HirAttribute {
    fn from(attr: &syl_syntax::Attribute) -> Self {
        Self {
            name: attr.name.clone(),
            args: attr.args.iter().map(HirBodyExpr::from_syntax).collect(),
            span: attr.span,
        }
    }
}

impl HirAttribute {
    fn summary_count(&self) -> usize {
        self.name.len()
            + self.args.iter().map(|arg| arg.span().start).sum::<usize>()
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirViewDecl {
    pub name: String,
    pub fields: Vec<HirViewField>,
    pub span: Span,
}

impl From<&syl_syntax::ViewDecl> for HirViewDecl {
    fn from(view: &syl_syntax::ViewDecl) -> Self {
        Self {
            name: view.name.clone(),
            fields: view.fields.iter().map(HirViewField::from).collect(),
            span: view.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirViewField {
    pub direction: HirViewDirection,
    pub name: String,
    pub span: Span,
}

impl From<&syl_syntax::ViewField> for HirViewField {
    fn from(field: &syl_syntax::ViewField) -> Self {
        Self {
            direction: HirViewDirection::from(&field.dir),
            name: field.name.clone(),
            span: field.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirPortDecl {
    pub name: String,
    pub direction: HirPortDirection,
    pub ty: MirTypeRef,
    pub drive: HirDriveCapability,
    pub span: Span,
}

impl From<&PortDecl> for HirPortDecl {
    fn from(port: &PortDecl) -> Self {
        Self {
            name: port.name.clone(),
            direction: HirPortDirection::from(port),
            ty: MirTypeRef::from(&port.ty),
            drive: HirDriveCapability::from(&port.drive),
            span: port.span,
        }
    }
}

impl HirPortDecl {
    fn summary_count(&self) -> usize {
        let direction = match self.direction {
            HirPortDirection::In => 1,
            HirPortDirection::InOut => 2,
            HirPortDirection::Out => 3,
        };
        let drive = match self.drive {
            HirDriveCapability::ReadOnly => 1,
            HirDriveCapability::ReadWrite => 2,
            HirDriveCapability::WriteOnly => 3,
        };
        self.name.len() + direction + self.ty.span().start + drive + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirConstItem {
    pub name: String,
    pub ty: Option<MirTypeRef>,
    pub value: HirBodyExpr,
    pub span: Span,
}

impl From<&ConstItem> for HirConstItem {
    fn from(item: &ConstItem) -> Self {
        Self {
            name: item.name.clone(),
            ty: item.ty.as_ref().map(MirTypeRef::from),
            value: HirBodyExpr::from_syntax(&item.value),
            span: item.span,
        }
    }
}

impl HirConstItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.span.start
            + self
                .ty
                .as_ref()
                .map(MirTypeRef::span)
                .map_or(0, |span| span.start)
            + self.value.span().start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirFnItem {
    pub name: String,
    pub params: Vec<HirSignatureParam>,
    pub ret_ty: Option<HirReturnType>,
    pub body: HirBlock,
    pub span: Span,
}

impl From<&FnItem> for HirFnItem {
    fn from(item: &FnItem) -> Self {
        Self {
            name: item.name.clone(),
            params: item.params.iter().map(HirSignatureParam::from).collect(),
            ret_ty: item.ret_ty.as_ref().map(HirReturnType::from),
            body: HirBlock::from_syntax(&item.body),
            span: item.span,
        }
    }
}

impl HirFnItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.params.len()
            + self
                .ret_ty
                .as_ref()
                .map_or(0, |ret_ty| ret_ty.ty.span().start)
            + self.body.span.start
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirEnumItem {
    pub name: String,
    pub variants: Vec<HirEnumVariantDecl>,
    pub span: Span,
}

impl From<&EnumItem> for HirEnumItem {
    fn from(item: &EnumItem) -> Self {
        Self {
            name: item.name.clone(),
            variants: item.variants.iter().map(HirEnumVariantDecl::from).collect(),
            span: item.span,
        }
    }
}

impl HirEnumItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self
                .variants
                .iter()
                .map(HirEnumVariantDecl::summary_count)
                .sum::<usize>()
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirBundleItem {
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub fields: Vec<HirFieldDecl>,
    pub attrs: Vec<HirAttribute>,
    pub span: Span,
}

impl From<&BundleItem> for HirBundleItem {
    fn from(item: &BundleItem) -> Self {
        Self {
            name: item.name.clone(),
            generics: item
                .generics
                .iter()
                .map(HirSignatureGenericParam::from)
                .collect(),
            fields: item.fields.iter().map(HirFieldDecl::from).collect(),
            attrs: item.attrs.iter().map(HirAttribute::from).collect(),
            span: item.span,
        }
    }
}

impl HirBundleItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.generics.len()
            + self.fields.len()
            + self
                .attrs
                .iter()
                .map(HirAttribute::summary_count)
                .sum::<usize>()
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirInterfaceItem {
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub fields: Vec<HirFieldDecl>,
    pub views: Vec<HirViewDecl>,
    pub span: Span,
}

impl From<&InterfaceItem> for HirInterfaceItem {
    fn from(item: &InterfaceItem) -> Self {
        Self {
            name: item.name.clone(),
            generics: item
                .generics
                .iter()
                .map(HirSignatureGenericParam::from)
                .collect(),
            fields: item.fields.iter().map(HirFieldDecl::from).collect(),
            views: item.views.iter().map(HirViewDecl::from).collect(),
            span: item.span,
        }
    }
}

impl HirInterfaceItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.generics.len()
            + self.fields.len()
            + self.views.len()
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirMapItem {
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub params: Vec<HirSignatureParam>,
    pub ret_ty: Option<HirReturnType>,
    pub body: HirBodyExpr,
    pub span: Span,
}

impl From<&MapItem> for HirMapItem {
    fn from(item: &MapItem) -> Self {
        Self {
            name: item.name.clone(),
            generics: item
                .generics
                .iter()
                .map(HirSignatureGenericParam::from)
                .collect(),
            params: item.params.iter().map(HirSignatureParam::from).collect(),
            ret_ty: item.ret_ty.as_ref().map(HirReturnType::from),
            body: HirBodyExpr::from_syntax(&item.body),
            span: item.span,
        }
    }
}

impl HirMapItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.generics.len()
            + self.params.len()
            + self
                .ret_ty
                .as_ref()
                .map_or(0, |ret_ty| ret_ty.ty.span().start)
            + self.body.span().start
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirCallableItem {
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub params: Vec<HirSignatureParam>,
    pub ports: Vec<HirPortDecl>,
    pub result: Option<HirSignatureResultBinding>,
    pub body: HirBlock,
    pub span: Span,
}

impl From<&syl_syntax::CallableItem> for HirCallableItem {
    fn from(item: &syl_syntax::CallableItem) -> Self {
        Self {
            name: item.name.clone(),
            generics: item
                .generics
                .iter()
                .map(HirSignatureGenericParam::from)
                .collect(),
            params: item.params.iter().map(HirSignatureParam::from).collect(),
            ports: item.ports.iter().map(HirPortDecl::from).collect(),
            result: item.result.as_ref().map(HirSignatureResultBinding::from),
            body: HirBlock::from_syntax(&item.body),
            span: item.span,
        }
    }
}

impl HirCallableItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.generics.len()
            + self.params.len()
            + self
                .ports
                .iter()
                .map(HirPortDecl::summary_count)
                .sum::<usize>()
            + self.result.as_ref().map_or(0, |result| result.span.start)
            + self.body.span.start
            + self.span.start
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirExternModuleItem {
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub params: Vec<HirSignatureParam>,
    pub ports: Vec<HirPortDecl>,
    pub result: Option<HirSignatureResultBinding>,
    pub span: Span,
}

impl From<&ExternModuleItem> for HirExternModuleItem {
    fn from(item: &ExternModuleItem) -> Self {
        Self {
            name: item.name.clone(),
            generics: item
                .generics
                .iter()
                .map(HirSignatureGenericParam::from)
                .collect(),
            params: item.params.iter().map(HirSignatureParam::from).collect(),
            ports: item.ports.iter().map(HirPortDecl::from).collect(),
            result: item.result.as_ref().map(HirSignatureResultBinding::from),
            span: item.span,
        }
    }
}

impl HirExternModuleItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.generics.len()
            + self.params.len()
            + self
                .ports
                .iter()
                .map(HirPortDecl::summary_count)
                .sum::<usize>()
            + self.result.as_ref().map_or(0, |result| result.span.start)
            + self.span.start
    }
}
