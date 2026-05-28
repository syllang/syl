//! Best-effort semantic sketch for the HIR design.
//!
//! `semantic_summary_count()` computes a single `usize` that changes when
//! the *semantic content* of the design changes (definitions, locals, exprs,
//! type refs, etc.). It is NOT a hash or a cryptographic digest — it's a
//! best-effort ordering-friendly sum for coarse change observation.
//!
//! **How it works:** Each type contributes a mix of name lengths, span
//! positions, enum discriminants, and arena IDs. These are chosen to be:
//! 1. Cheap to compute — no heap allocations, just integer arithmetic.
//! 2. Sensitive to many semantic changes — adding a field, renaming a def, or
//!    inserting a statement changes the count.
//!
//! **Limitations:** Collisions are possible, and the sketch is not stable across
//! source-preserving layout changes because it incorporates span offsets.

use super::{
    HirBundleItem, HirCallable, HirConstItem, HirDef, HirDefKind, HirDesign, HirEnumItem, HirExpr,
    HirFieldAccess, HirFnItem, HirInterfaceItem, HirLocal, HirLocalKind, HirMapItem, HirMemberDecl,
    HirMemberKind, HirTypeRef,
};

// Stable internal summary tags for the definition fingerprint.
//
// These values are part of the cache contract for `summary_count()`: changing
// them changes the semantic fingerprint for every definition. The gap between
// `Cell` and `ExternCell` is intentional so a future variant can be inserted
// without renumbering the existing tags.
const SUMMARY_TAG_DEF_CONST: usize = 1;
const SUMMARY_TAG_DEF_FN: usize = 2;
const SUMMARY_TAG_DEF_ENUM: usize = 3;
const SUMMARY_TAG_DEF_BUNDLE: usize = 4;
const SUMMARY_TAG_DEF_INTERFACE: usize = 5;
const SUMMARY_TAG_DEF_MAP: usize = 6;
const SUMMARY_TAG_DEF_CELL: usize = 7;
const SUMMARY_TAG_DEF_EXTERN_CELL: usize = 9;

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
            Self::Const => SUMMARY_TAG_DEF_CONST,
            Self::Fn => SUMMARY_TAG_DEF_FN,
            Self::Enum => SUMMARY_TAG_DEF_ENUM,
            Self::Bundle => SUMMARY_TAG_DEF_BUNDLE,
            Self::Interface => SUMMARY_TAG_DEF_INTERFACE,
            Self::Map => SUMMARY_TAG_DEF_MAP,
            Self::Cell => SUMMARY_TAG_DEF_CELL,
            Self::ExternCell => SUMMARY_TAG_DEF_EXTERN_CELL,
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

#[cfg(test)]
mod tests;
