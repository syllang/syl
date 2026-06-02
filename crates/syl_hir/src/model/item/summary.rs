use super::{
    HirAttribute, HirBundleItem, HirCallableItem, HirConstItem, HirDriveCapability, HirEnumItem,
    HirEnumLayout, HirEnumVariantDecl, HirExternCellItem, HirFnItem, HirInterfaceItem, HirMapItem,
    HirPortDecl, HirPortDirection, HirStructItem, MirTypeRef,
};

impl HirEnumLayout {
    pub(super) fn summary_count(self) -> usize {
        match self {
            Self::Ordinal => 1,
            Self::Flags => 2,
            Self::OneHot => 3,
        }
    }
}

impl HirEnumVariantDecl {
    pub(super) fn summary_count(&self) -> usize {
        self.name.len()
            + self.value.as_ref().map_or(0, |value| value.span().start)
            + self.span.start
    }
}

impl HirAttribute {
    pub(super) fn summary_count(&self) -> usize {
        self.name.len()
            + self.args.iter().map(|arg| arg.span().start).sum::<usize>()
            + self.span.start
    }
}

impl HirPortDecl {
    pub(super) fn summary_count(&self) -> usize {
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

impl HirEnumItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len()
            + self.width.as_ref().map_or(0, |width| width.span().start)
            + self.layout.summary_count()
            + self
                .variants
                .iter()
                .map(HirEnumVariantDecl::summary_count)
                .sum::<usize>()
            + self.span.start
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

impl HirStructItem {
    pub(crate) fn summary_count(&self) -> usize {
        self.name.len() + self.generics.len() + self.fields.len() + self.span.start
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

impl HirExternCellItem {
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
