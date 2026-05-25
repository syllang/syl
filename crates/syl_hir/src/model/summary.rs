use super::{
    HirBundleItem, HirCallable, HirConstItem, HirDef, HirDefKind, HirDesign, HirEnumItem, HirExpr,
    HirFieldAccess, HirFnItem, HirInterfaceItem, HirLocal, HirLocalKind, HirMapItem, HirMemberDecl,
    HirMemberKind, HirTypeRef,
};

impl HirDesign {
    pub fn semantic_summary_count(&self) -> usize {
        self.package_sum()
            + self.import_sum()
            + self.defs.iter().map(HirDef::summary_count).sum::<usize>()
            + self.item_sum()
            + self
                .locals
                .iter()
                .map(HirLocal::summary_count)
                .sum::<usize>()
            + self.exprs.iter().map(HirExpr::summary_count).sum::<usize>()
            + self
                .field_accesses
                .iter()
                .map(HirFieldAccess::summary_count)
                .sum::<usize>()
            + self
                .type_refs
                .iter()
                .map(HirTypeRef::summary_count)
                .sum::<usize>()
            + self
                .enum_variants
                .values()
                .map(|variant| variant.summary_count())
                .sum::<usize>()
            + self
                .member_decls
                .iter()
                .map(HirMemberDecl::summary_count)
                .sum::<usize>()
    }

    fn package_sum(&self) -> usize {
        self.packages
            .iter()
            .map(|package| package.id.get() + package.path.len() + package.span.start)
            .sum()
    }

    fn import_sum(&self) -> usize {
        self.imports
            .iter()
            .map(|import| import.path.len() + import.package_path.len() + import.span.start)
            .sum()
    }

    fn item_sum(&self) -> usize {
        self.consts
            .values()
            .map(HirConstItem::summary_count)
            .sum::<usize>()
            + self
                .fns
                .values()
                .map(HirFnItem::summary_count)
                .sum::<usize>()
            + self
                .enums
                .values()
                .map(HirEnumItem::summary_count)
                .sum::<usize>()
            + self
                .bundles
                .values()
                .map(HirBundleItem::summary_count)
                .sum::<usize>()
            + self
                .interfaces
                .values()
                .map(HirInterfaceItem::summary_count)
                .sum::<usize>()
            + self
                .maps
                .values()
                .map(HirMapItem::summary_count)
                .sum::<usize>()
            + self
                .callables
                .values()
                .map(HirCallable::summary_count)
                .sum::<usize>()
    }
}

impl HirDef {
    pub(super) fn summary_count(&self) -> usize {
        self.id.get()
            + self.name.len()
            + self.canonical_path.len()
            + self.kind.summary_count()
            + self.span.start
    }
}

impl HirDefKind {
    pub(super) fn summary_count(&self) -> usize {
        match self {
            Self::Const => 1,
            Self::Fn => 2,
            Self::Enum => 3,
            Self::Bundle => 4,
            Self::Interface => 5,
            Self::Map => 6,
            Self::Cell => 7,
            Self::Module => 8,
            Self::ExternModule => 9,
        }
    }
}

impl HirLocal {
    pub(super) fn summary_count(&self) -> usize {
        self.id.get()
            + self.owner.get()
            + self.name.len()
            + self.kind.summary_count()
            + self.span.start
    }
}

impl HirLocalKind {
    pub(super) fn summary_count(self) -> usize {
        match self {
            Self::Generic => 1,
            Self::Param => 2,
            Self::Result => 3,
            Self::Const => 4,
            Self::Let => 5,
            Self::Var => 6,
            Self::Signal => 7,
            Self::Reg => 8,
            Self::Instance => 9,
            Self::Loop => 10,
        }
    }
}

impl HirExpr {
    pub(super) fn summary_count(&self) -> usize {
        self.id.get() + self.owner.get() + self.span.start + self.span.end
    }
}

impl HirFieldAccess {
    pub(super) fn summary_count(&self) -> usize {
        self.owner.get() + self.field.len() + self.span.start + self.base.span().end
    }
}

impl HirTypeRef {
    pub(super) fn summary_count(&self) -> usize {
        self.owner.get() + self.span.start + self.ty.span().end
    }
}

impl HirMemberDecl {
    pub(super) fn summary_count(&self) -> usize {
        self.owner.get() + self.name.len() + self.kind.summary_count() + self.span.start
    }
}

impl HirMemberKind {
    pub(super) fn summary_count(&self) -> usize {
        match self {
            Self::Field { ty } => 1 + ty.span().start,
            Self::View => 2,
            Self::ViewField { view } => 3 + view.len(),
        }
    }
}
