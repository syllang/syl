use super::{ElabPortDirection, ElabViewDirection, body::ElabBlock, body::ElabExpr};
use crate::{
    mir::MirTypeRef,
    source::{
        HirBundleItem, HirCallable, HirCallableItem, HirConstItem, HirEnumItem, HirEnumVariantKey,
        HirExternCellItem, HirFieldDecl, HirInterfaceItem, HirSignatureGenericParam,
        HirSignatureParam, HirSignatureResultBinding, HirViewDecl, HirViewField,
    },
};
use syl_hir::DefId;
use syl_span::Span;

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabSignatureGenericParam {
    pub(crate) doc: Option<String>,
    pub(crate) name: String,
    pub(crate) kind: Option<MirTypeRef>,
    pub(crate) default: Option<ElabExpr>,
}

impl From<&HirSignatureGenericParam> for ElabSignatureGenericParam {
    fn from(value: &HirSignatureGenericParam) -> Self {
        Self {
            doc: value.doc.clone(),
            name: value.name.clone(),
            kind: value.kind.clone(),
            default: value.default.as_ref().map(ElabExpr::from),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabSignatureParam {
    pub(crate) doc: Option<String>,
    pub(crate) name: String,
    pub(crate) direction: ElabPortDirection,
    pub(crate) ty: MirTypeRef,
    pub(crate) span: Span,
}

impl From<&HirSignatureParam> for ElabSignatureParam {
    fn from(value: &HirSignatureParam) -> Self {
        Self {
            doc: value.doc.clone(),
            name: value.name.clone(),
            direction: ElabPortDirection::from(value.direction),
            ty: value.ty.clone(),
            span: value.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabSignatureResultBinding {
    pub(crate) doc: Option<String>,
    pub(crate) name: String,
    pub(crate) ty: MirTypeRef,
    pub(crate) span: Span,
}

impl From<&HirSignatureResultBinding> for ElabSignatureResultBinding {
    fn from(value: &HirSignatureResultBinding) -> Self {
        Self {
            doc: value.doc.clone(),
            name: value.name.clone(),
            ty: value.ty.clone(),
            span: value.span,
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabConstItem {
    pub(crate) value: ElabExpr,
}

impl From<&HirConstItem> for ElabConstItem {
    fn from(value: &HirConstItem) -> Self {
        Self {
            value: ElabExpr::from(&value.value),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabEnumItem {
    pub(crate) width: Option<MirTypeRef>,
    pub(crate) max_value: u64,
}

impl ElabEnumItem {
    pub(crate) fn new(value: &HirEnumItem, max_value: u64) -> Self {
        Self {
            width: value.width.clone(),
            max_value,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub(super) struct ElabEnumVariantKey {
    enum_def: DefId,
    name: String,
}

impl ElabEnumVariantKey {
    pub(super) fn new(enum_def: DefId, name: impl Into<String>) -> Self {
        Self {
            enum_def,
            name: name.into(),
        }
    }
}

impl From<&HirEnumVariantKey> for ElabEnumVariantKey {
    fn from(value: &HirEnumVariantKey) -> Self {
        Self::new(value.enum_def, &value.name)
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabFieldDecl {
    pub(crate) name: String,
    pub(crate) ty: MirTypeRef,
}

impl From<&HirFieldDecl> for ElabFieldDecl {
    fn from(value: &HirFieldDecl) -> Self {
        Self {
            name: value.name.clone(),
            ty: value.ty.clone(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabViewField {
    pub(crate) direction: ElabViewDirection,
    pub(crate) name: String,
}

impl From<&HirViewField> for ElabViewField {
    fn from(value: &HirViewField) -> Self {
        Self {
            direction: ElabViewDirection::from(value.direction),
            name: value.name.clone(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabViewDecl {
    pub(crate) name: String,
    pub(crate) fields: Vec<ElabViewField>,
}

impl From<&HirViewDecl> for ElabViewDecl {
    fn from(value: &HirViewDecl) -> Self {
        Self {
            name: value.name.clone(),
            fields: value.fields.iter().map(ElabViewField::from).collect(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabBundleItem {
    pub(crate) generics: Vec<ElabSignatureGenericParam>,
    pub(crate) fields: Vec<ElabFieldDecl>,
}

impl From<&HirBundleItem> for ElabBundleItem {
    fn from(value: &HirBundleItem) -> Self {
        Self {
            generics: value
                .generics
                .iter()
                .map(ElabSignatureGenericParam::from)
                .collect(),
            fields: value.fields.iter().map(ElabFieldDecl::from).collect(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabInterfaceItem {
    pub(crate) generics: Vec<ElabSignatureGenericParam>,
    pub(crate) fields: Vec<ElabFieldDecl>,
    pub(crate) views: Vec<ElabViewDecl>,
}

impl From<&HirInterfaceItem> for ElabInterfaceItem {
    fn from(value: &HirInterfaceItem) -> Self {
        Self {
            generics: value
                .generics
                .iter()
                .map(ElabSignatureGenericParam::from)
                .collect(),
            fields: value.fields.iter().map(ElabFieldDecl::from).collect(),
            views: value.views.iter().map(ElabViewDecl::from).collect(),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) enum ElabCallable {
    Cell(ElabCallableItem),
    Extern(ElabExternCellItem),
}

impl ElabCallable {
    pub(crate) fn generics(&self) -> &[ElabSignatureGenericParam] {
        match self {
            Self::Cell(item) => &item.generics,
            Self::Extern(item) => &item.generics,
        }
    }

    pub(crate) fn params(&self) -> &[ElabSignatureParam] {
        match self {
            Self::Cell(item) => &item.params,
            Self::Extern(item) => &item.params,
        }
    }

    pub(crate) fn result(&self) -> Option<&ElabSignatureResultBinding> {
        match self {
            Self::Cell(item) => item.result.as_ref(),
            Self::Extern(item) => item.result.as_ref(),
        }
    }
}

impl From<&HirCallable> for ElabCallable {
    fn from(value: &HirCallable) -> Self {
        match value {
            HirCallable::Cell(item) => Self::Cell(ElabCallableItem::from(item)),
            HirCallable::Extern(item) => Self::Extern(ElabExternCellItem::from(item)),
            _ => unreachable!("unknown HIR callable reached elaboration IR"),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabCallableItem {
    pub(crate) doc: Option<String>,
    pub(crate) name: String,
    pub(crate) generics: Vec<ElabSignatureGenericParam>,
    pub(crate) params: Vec<ElabSignatureParam>,
    pub(crate) result: Option<ElabSignatureResultBinding>,
    pub(crate) body: ElabBlock,
}

impl From<&HirCallableItem> for ElabCallableItem {
    fn from(value: &HirCallableItem) -> Self {
        Self {
            doc: value.doc.clone(),
            name: value.name.clone(),
            generics: value
                .generics
                .iter()
                .map(ElabSignatureGenericParam::from)
                .collect(),
            params: value.params.iter().map(ElabSignatureParam::from).collect(),
            result: value.result.as_ref().map(ElabSignatureResultBinding::from),
            body: ElabBlock::from(&value.body),
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub(crate) struct ElabExternCellItem {
    pub(crate) doc: Option<String>,
    pub(crate) name: String,
    pub(crate) generics: Vec<ElabSignatureGenericParam>,
    pub(crate) params: Vec<ElabSignatureParam>,
    pub(crate) result: Option<ElabSignatureResultBinding>,
}

impl From<&HirExternCellItem> for ElabExternCellItem {
    fn from(value: &HirExternCellItem) -> Self {
        Self {
            doc: value.doc.clone(),
            name: value.name.clone(),
            generics: value
                .generics
                .iter()
                .map(ElabSignatureGenericParam::from)
                .collect(),
            params: value.params.iter().map(ElabSignatureParam::from).collect(),
            result: value.result.as_ref().map(ElabSignatureResultBinding::from),
        }
    }
}
