use crate::name::HirPath;
use crate::resolution::HirResolution;
use crate::{DefId, ExprId, LocalId, PackageId};
use std::collections::BTreeMap;
use syl_span::Span;

mod body;
mod callable;
mod enum_variant;
mod item;
mod labels;
mod summary;
mod type_ref;

pub use type_ref::{MirBinaryOp, MirConstExpr, MirPattern, MirSelectMode, MirTypeRef, MirUnaryOp};

pub use body::{
    HirBlock, HirExpr as HirBodyExpr, HirExprNode, HirInstArg, HirMatchArm, HirNamedExpr,
    HirRegReset, HirSelectArm, HirStmt,
};
pub use callable::HirCallable;
pub use enum_variant::{HirEnumVariant, HirEnumVariantKey};
pub use item::{
    HirAttribute, HirBundleItem, HirCallableItem, HirConstItem, HirDriveCapability, HirEnumItem,
    HirExternModuleItem, HirFieldDecl, HirFnItem, HirInterfaceItem, HirMapItem, HirPortDecl,
    HirPortDirection, HirSignatureGenericParam, HirSignatureParam, HirSignatureResultBinding,
    HirViewDecl, HirViewDirection, HirViewField,
};

#[non_exhaustive]
pub struct HirDesign {
    pub packages: Vec<HirPackage>,
    pub imports: Vec<HirImport>,
    pub defs: Vec<HirDef>,
    pub def_names: BTreeMap<String, Vec<DefId>>,
    pub canonical_def_names: BTreeMap<HirPath, DefId>,
    pub locals: Vec<HirLocal>,
    pub exprs: Vec<HirExpr>,
    pub field_accesses: Vec<HirFieldAccess>,
    pub type_refs: Vec<HirTypeRef>,
    pub member_decls: Vec<HirMemberDecl>,
    pub expr_resolutions: BTreeMap<ExprId, HirResolution>,
    pub consts: BTreeMap<DefId, HirConstItem>,
    pub fns: BTreeMap<DefId, HirFnItem>,
    pub enums: BTreeMap<DefId, HirEnumItem>,
    pub enum_variants: BTreeMap<HirEnumVariantKey, HirEnumVariant>,
    pub bundles: BTreeMap<DefId, HirBundleItem>,
    pub interfaces: BTreeMap<DefId, HirInterfaceItem>,
    pub maps: BTreeMap<DefId, HirMapItem>,
    pub callables: BTreeMap<DefId, HirCallable>,
}

impl HirDesign {
    pub fn empty() -> Self {
        Self {
            packages: Vec::new(),
            imports: Vec::new(),
            defs: Vec::new(),
            def_names: BTreeMap::new(),
            canonical_def_names: BTreeMap::new(),
            locals: Vec::new(),
            exprs: Vec::new(),
            field_accesses: Vec::new(),
            type_refs: Vec::new(),
            member_decls: Vec::new(),
            expr_resolutions: BTreeMap::new(),
            consts: BTreeMap::new(),
            fns: BTreeMap::new(),
            enums: BTreeMap::new(),
            enum_variants: BTreeMap::new(),
            bundles: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            maps: BTreeMap::new(),
            callables: BTreeMap::new(),
        }
    }

    pub fn def_name(&self, id: DefId) -> Option<&str> {
        self.defs.get(id.get()).map(|def| def.name.as_str())
    }

    pub fn visible_def_ids(&self, owner: DefId) -> Vec<DefId> {
        let Some(package) = self.package_path_for_def(owner) else {
            return Vec::new();
        };
        let mut defs = self
            .defs
            .iter()
            .filter(|def| def.canonical_path.is_direct_child_of(&package))
            .map(|def| def.id)
            .collect::<Vec<_>>();
        defs.extend(
            self.imports
                .iter()
                .filter(|import| import.package_path == package)
                .filter_map(|import| {
                    self.canonical_def_names
                        .get(&HirPath::new(import.path.clone()))
                })
                .copied(),
        );
        defs
    }

    pub fn source_def_ids(&self, source: syl_span::SourceId) -> Vec<DefId> {
        self.defs
            .iter()
            .filter(|def| def.span.source == source)
            .map(|def| def.id)
            .collect()
    }

    pub fn import_def_at(&self, span: Span) -> Option<DefId> {
        self.imports
            .iter()
            .filter(|import| contains_span(import.span, span))
            .min_by_key(|import| span_width(import.span))
            .and_then(|import| {
                self.canonical_def_names
                    .get(&HirPath::new(import.path.clone()))
                    .copied()
            })
    }

    pub fn member_field_def_at(&self, owner: DefId, span: Span) -> Option<&HirMemberDecl> {
        self.member_decls
            .iter()
            .filter(|member| member.owner == owner && contains_span(member.span, span))
            .min_by_key(|member| span_width(member.span))
    }

    pub fn member_decl_definition_at(&self, span: Span) -> Option<&HirMemberDecl> {
        self.member_decls
            .iter()
            .filter(|member| contains_span(member.span, span))
            .min_by_key(|member| span_width(member.span))
    }

    pub fn type_ref_at(&self, owner: DefId, span: Span) -> Option<&HirTypeRef> {
        self.type_refs
            .iter()
            .filter(|type_ref| type_ref.owner == owner && contains_span(type_ref.span, span))
            .min_by_key(|type_ref| span_width(type_ref.span))
    }

    pub fn view_def_for_type_ref(
        &self,
        owner: DefId,
        ty: &MirTypeRef,
        span: Span,
    ) -> Option<&HirMemberDecl> {
        let (base, view) = ty.view_select()?;
        if !contains_span(ty.span(), span) {
            return None;
        }
        let type_def = self.type_def_for_mir_type(owner, base)?;
        self.member_decls.iter().find(|member| {
            member.owner == type_def
                && member.name == view
                && matches!(member.kind, HirMemberKind::View)
        })
    }

    pub fn resolved_type_def_for_ref(&self, type_ref: &HirTypeRef) -> Option<DefId> {
        self.type_def_for_mir_type(type_ref.owner, &type_ref.ty)
    }

    pub fn member_completion_fields_at(&self, owner: DefId, _span: Span) -> Vec<&HirMemberDecl> {
        self.member_decls
            .iter()
            .filter(|member| owner == member.owner)
            .collect()
    }

    fn type_def_for_mir_type(&self, _owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
        if let Some(path) = ty.path() {
            if path.len() == 1 {
                return self
                    .def_names
                    .get(&path[0])
                    .and_then(|defs| defs.first())
                    .copied();
            }
            return self
                .canonical_def_names
                .get(&HirPath::new(path.to_vec()))
                .copied();
        }
        if let Some(base) = ty.generic_base() {
            return self.type_def_for_mir_type(_owner, base);
        }
        if let Some((base, _)) = ty.view_select() {
            return self.type_def_for_mir_type(_owner, base);
        }
        if let Some((_, elem)) = ty.array() {
            return self.type_def_for_mir_type(_owner, elem);
        }
        None
    }

    fn package_path_for_def(&self, owner: DefId) -> Option<HirPath> {
        self.defs
            .get(owner.get())
            .map(|def| def.canonical_path.parent())
    }
}

fn contains_span(container: Span, cursor: Span) -> bool {
    container.source == cursor.source
        && container.start <= cursor.start
        && cursor.end <= container.end
}

fn span_width(span: Span) -> usize {
    span.end.saturating_sub(span.start)
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirPackage {
    pub id: PackageId,
    pub path: Vec<String>,
    pub span: Span,
}

impl HirPackage {
    pub fn new(id: PackageId, path: Vec<String>, span: Span) -> Self {
        Self { id, path, span }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirImport {
    pub path: Vec<String>,
    pub package_path: HirPath,
    pub span: Span,
}

impl HirImport {
    pub fn new(path: Vec<String>, package_path: HirPath, span: Span) -> Self {
        Self {
            path,
            package_path,
            span,
        }
    }
}

#[non_exhaustive]
pub struct HirDef {
    pub id: DefId,
    pub name: String,
    pub canonical_path: HirPath,
    pub kind: HirDefKind,
    pub span: Span,
}

impl HirDef {
    pub fn new(
        id: DefId,
        name: String,
        canonical_path: HirPath,
        kind: HirDefKind,
        span: Span,
    ) -> Self {
        Self {
            id,
            name,
            canonical_path,
            kind,
            span,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum HirDefKind {
    Const,
    Fn,
    Enum,
    Bundle,
    Interface,
    Map,
    Cell,
    Module,
    ExternModule,
}

#[non_exhaustive]
pub struct HirLocal {
    pub id: LocalId,
    pub owner: DefId,
    pub name: String,
    pub kind: HirLocalKind,
    pub span: Span,
}

impl HirLocal {
    pub fn new(id: LocalId, owner: DefId, name: String, kind: HirLocalKind, span: Span) -> Self {
        Self {
            id,
            owner,
            name,
            kind,
            span,
        }
    }
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub enum HirLocalKind {
    Generic,
    Param,
    Result,
    Const,
    Let,
    Var,
    Alias,
    Signal,
    Reg,
    Instance,
    Loop,
}

#[derive(Clone, Copy)]
#[non_exhaustive]
pub struct HirExpr {
    pub id: ExprId,
    pub owner: DefId,
    pub span: Span,
}

impl HirExpr {
    pub fn new(id: ExprId, owner: DefId, span: Span) -> Self {
        Self { id, owner, span }
    }
}

#[non_exhaustive]
pub struct HirFieldAccess {
    pub owner: DefId,
    pub base: HirBodyExpr,
    pub field: String,
    pub span: Span,
}

impl HirFieldAccess {
    pub fn new(owner: DefId, base: HirBodyExpr, field: String, span: Span) -> Self {
        Self {
            owner,
            base,
            field,
            span,
        }
    }
}

#[non_exhaustive]
pub struct HirMemberDecl {
    pub owner: DefId,
    pub name: String,
    pub kind: HirMemberKind,
    pub span: Span,
}

impl HirMemberDecl {
    pub fn new(owner: DefId, name: String, kind: HirMemberKind, span: Span) -> Self {
        Self {
            owner,
            name,
            kind,
            span,
        }
    }
}

#[non_exhaustive]
pub enum HirMemberKind {
    Field { ty: MirTypeRef },
    View,
    ViewField { view: String },
}

impl HirMemberKind {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Field { .. } => "field",
            Self::View => "view",
            Self::ViewField { .. } => "view field",
        }
    }
}

#[derive(Clone)]
#[non_exhaustive]
pub struct HirTypeRef {
    pub owner: DefId,
    pub ty: MirTypeRef,
    pub span: Span,
}

impl HirTypeRef {
    pub fn new(owner: DefId, ty: MirTypeRef) -> Self {
        Self {
            owner,
            span: ty.span(),
            ty,
        }
    }

    pub fn definition(&self) -> Option<DefId> {
        None
    }
}
