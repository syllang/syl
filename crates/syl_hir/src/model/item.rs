use super::{HirBlock, HirBodyExpr, MirTypeRef};
use crate::LocalId;
use strum_macros::IntoStaticStr;
use syl_span::Span;
use syl_syntax::{
    BundleItem, ConstItem, DriveCapability, EnumItem, EnumLayout as SyntaxEnumLayout,
    ExternCellItem, FieldDecl, FnItem, GenericParam, InterfaceItem, MapItem, Param, ParamDirection,
    PortDecl, TypeExpr, ViewDirection,
};

mod summary;

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HirPortDirection {
    #[strum(serialize = "in")]
    In,
    #[strum(serialize = "inout")]
    InOut,
    #[strum(serialize = "out")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HirDriveCapability {
    #[strum(serialize = "read only")]
    ReadOnly,
    #[strum(serialize = "read write")]
    ReadWrite,
    #[strum(serialize = "write only")]
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
    pub doc: Option<String>,
    pub id: Option<LocalId>,
    pub name: String,
    pub kind: Option<MirTypeRef>,
    pub default: Option<HirBodyExpr>,
    pub span: Span,
}

impl From<&GenericParam> for HirSignatureGenericParam {
    fn from(param: &GenericParam) -> Self {
        Self {
            doc: param.doc.clone(),
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
    pub doc: Option<String>,
    pub id: Option<LocalId>,
    pub name: String,
    pub direction: HirPortDirection,
    pub ty: MirTypeRef,
    pub role: HirParamRole,
    pub span: Span,
}

impl HirSignatureParam {
    pub fn is_receiver(&self) -> bool {
        matches!(self.role, HirParamRole::Receiver)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirParamRole {
    Ordinary,
    Receiver,
}

impl From<&Param> for HirParamRole {
    fn from(param: &Param) -> Self {
        if param.is_receiver() {
            Self::Receiver
        } else {
            Self::Ordinary
        }
    }
}

impl From<&Param> for HirSignatureParam {
    fn from(param: &Param) -> Self {
        Self {
            doc: param.doc.clone(),
            id: None,
            name: param.name.clone(),
            direction: HirPortDirection::from(param.dir.as_ref()),
            ty: MirTypeRef::from(&param.ty),
            role: HirParamRole::from(param),
            span: param.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirSignatureResultBinding {
    pub doc: Option<String>,
    pub id: Option<LocalId>,
    pub name: String,
    pub ty: MirTypeRef,
    pub drive: HirDriveCapability,
    pub span: Span,
}

impl From<&syl_syntax::ResultBinding> for HirSignatureResultBinding {
    fn from(result: &syl_syntax::ResultBinding) -> Self {
        Self {
            doc: result.doc.clone(),
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HirViewDirection {
    #[strum(serialize = "in")]
    In,
    #[strum(serialize = "inout")]
    InOut,
    #[strum(serialize = "out")]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
pub enum HirEnumLayout {
    #[strum(serialize = "ordinal")]
    Ordinal,
    #[strum(serialize = "flags")]
    Flags,
    #[strum(serialize = "onehot")]
    OneHot,
}

impl From<&SyntaxEnumLayout> for HirEnumLayout {
    fn from(layout: &SyntaxEnumLayout) -> Self {
        match layout {
            SyntaxEnumLayout::Flags => Self::Flags,
            SyntaxEnumLayout::OneHot => Self::OneHot,
            SyntaxEnumLayout::Ordinal => Self::Ordinal,
            _ => Self::Ordinal,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirEnumVariantDecl {
    pub doc: Option<String>,
    pub name: String,
    pub value: Option<HirBodyExpr>,
    pub span: Span,
}

impl From<&syl_syntax::EnumVariant> for HirEnumVariantDecl {
    fn from(variant: &syl_syntax::EnumVariant) -> Self {
        Self {
            doc: variant.doc.clone(),
            name: variant.name.clone(),
            value: variant.value.as_ref().map(HirBodyExpr::from_syntax),
            span: variant.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirFieldDecl {
    pub doc: Option<String>,
    pub name: String,
    pub ty: MirTypeRef,
    pub span: Span,
}

impl From<&FieldDecl> for HirFieldDecl {
    fn from(field: &FieldDecl) -> Self {
        Self {
            doc: field.doc.clone(),
            name: field.name.clone(),
            ty: MirTypeRef::from(&field.ty),
            span: field.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirAttribute {
    pub doc: Option<String>,
    pub name: String,
    pub args: Vec<HirBodyExpr>,
    pub span: Span,
}

impl From<&syl_syntax::Attribute> for HirAttribute {
    fn from(attr: &syl_syntax::Attribute) -> Self {
        Self {
            doc: attr.doc.clone(),
            name: attr.name.clone(),
            args: attr.args.iter().map(HirBodyExpr::from_syntax).collect(),
            span: attr.span,
        }
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
    pub doc: Option<String>,
    pub direction: HirViewDirection,
    pub name: String,
    pub span: Span,
}

impl From<&syl_syntax::ViewField> for HirViewField {
    fn from(field: &syl_syntax::ViewField) -> Self {
        Self {
            doc: field.doc.clone(),
            direction: HirViewDirection::from(&field.dir),
            name: field.name.clone(),
            span: field.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirPortDecl {
    pub doc: Option<String>,
    pub name: String,
    pub direction: HirPortDirection,
    pub ty: MirTypeRef,
    pub drive: HirDriveCapability,
    pub span: Span,
}

impl From<&PortDecl> for HirPortDecl {
    fn from(port: &PortDecl) -> Self {
        Self {
            doc: port.doc.clone(),
            name: port.name.clone(),
            direction: HirPortDirection::from(port),
            ty: MirTypeRef::from(&port.ty),
            drive: HirDriveCapability::from(&port.drive),
            span: port.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirConstItem {
    pub doc: Option<String>,
    pub name: String,
    pub ty: Option<MirTypeRef>,
    pub value: HirBodyExpr,
    pub span: Span,
}

impl From<&ConstItem> for HirConstItem {
    fn from(item: &ConstItem) -> Self {
        Self {
            doc: item.doc.clone(),
            name: item.name.clone(),
            ty: item.ty.as_ref().map(MirTypeRef::from),
            value: HirBodyExpr::from_syntax(&item.value),
            span: item.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirFnItem {
    pub doc: Option<String>,
    pub name: String,
    pub params: Vec<HirSignatureParam>,
    pub ret_ty: Option<HirReturnType>,
    pub body: HirBlock,
    pub span: Span,
}

impl From<&FnItem> for HirFnItem {
    fn from(item: &FnItem) -> Self {
        Self {
            doc: item.doc.clone(),
            name: item.name.clone(),
            params: item.params.iter().map(HirSignatureParam::from).collect(),
            ret_ty: item.ret_ty.as_ref().map(HirReturnType::from),
            body: HirBlock::from_software_syntax(&item.body),
            span: item.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirEnumItem {
    pub doc: Option<String>,
    pub name: String,
    pub width: Option<MirTypeRef>,
    pub layout: HirEnumLayout,
    pub variants: Vec<HirEnumVariantDecl>,
    pub span: Span,
}

impl From<&EnumItem> for HirEnumItem {
    fn from(item: &EnumItem) -> Self {
        Self {
            doc: item.doc.clone(),
            name: item.name.clone(),
            width: item.width.as_ref().map(MirTypeRef::from),
            layout: HirEnumLayout::from(&item.layout),
            variants: item.variants.iter().map(HirEnumVariantDecl::from).collect(),
            span: item.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirBundleItem {
    pub doc: Option<String>,
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub fields: Vec<HirFieldDecl>,
    pub attrs: Vec<HirAttribute>,
    pub span: Span,
}

impl From<&BundleItem> for HirBundleItem {
    fn from(item: &BundleItem) -> Self {
        Self {
            doc: item.doc.clone(),
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

#[derive(Clone)]
#[non_exhaustive]
pub struct HirInterfaceItem {
    pub doc: Option<String>,
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub fields: Vec<HirFieldDecl>,
    pub views: Vec<HirViewDecl>,
    pub span: Span,
}

impl From<&InterfaceItem> for HirInterfaceItem {
    fn from(item: &InterfaceItem) -> Self {
        Self {
            doc: item.doc.clone(),
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

#[derive(Clone)]
#[non_exhaustive]
pub struct HirMapItem {
    pub doc: Option<String>,
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
            doc: item.doc.clone(),
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

#[derive(Clone)]
#[non_exhaustive]
pub struct HirCallableItem {
    pub doc: Option<String>,
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
            doc: item.doc.clone(),
            name: item.name.clone(),
            generics: item
                .generics
                .iter()
                .map(HirSignatureGenericParam::from)
                .collect(),
            params: item.params.iter().map(HirSignatureParam::from).collect(),
            ports: item.ports.iter().map(HirPortDecl::from).collect(),
            result: item.result.as_ref().map(HirSignatureResultBinding::from),
            body: HirBlock::from_hardware_syntax(&item.body),
            span: item.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirExternCellItem {
    pub doc: Option<String>,
    pub name: String,
    pub generics: Vec<HirSignatureGenericParam>,
    pub params: Vec<HirSignatureParam>,
    pub ports: Vec<HirPortDecl>,
    pub result: Option<HirSignatureResultBinding>,
    pub span: Span,
}

impl From<&ExternCellItem> for HirExternCellItem {
    fn from(item: &ExternCellItem) -> Self {
        Self {
            doc: item.doc.clone(),
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
