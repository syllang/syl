use crate::name::HirPath;
use crate::resolution::HirResolution;
use crate::{DefId, ExprId, LocalId, PackageId};
use std::collections::BTreeMap;
use strum_macros::IntoStaticStr;
use syl_span::{SourceId, Span};

mod body;
mod callable;
mod enum_variant;
mod item;
mod labels;
mod summary;
mod type_ref;

pub use type_ref::{MirBinaryOp, MirConstExpr, MirPattern, MirSelectMode, MirTypeRef, MirUnaryOp};

pub use body::{
    HirBlock, HirCallArg, HirExpr as HirBodyExpr, HirExprNode, HirMatchArm, HirNamedExpr,
    HirRegReset, HirSelectArm, HirStmt,
};
pub use callable::HirCallable;
pub use enum_variant::{HirEnumVariant, HirEnumVariantKey};
pub use item::{
    HirAttribute, HirBundleItem, HirCallableItem, HirConstItem, HirDriveCapability, HirEnumItem,
    HirEnumLayout, HirEnumVariantDecl, HirExternCellItem, HirFieldDecl, HirFnItem,
    HirInterfaceItem, HirMapItem, HirParamRole, HirPortDecl, HirPortDirection,
    HirSignatureGenericParam, HirSignatureParam, HirSignatureResultBinding, HirStructItem,
    HirViewDecl, HirViewDirection, HirViewField,
};

/// The complete HIR representation of a compiled Syl design.
///
/// `HirDesign` is the top-level container holding every definition,
/// expression, type reference, and resolution produced during semantic
/// analysis. It is the input to elaboration.
#[non_exhaustive]
pub struct HirDesign {
    pub packages: Vec<HirPackage>,
    pub module_docs: BTreeMap<SourceId, String>,
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
    pub extension_methods: HirExtensionMethodIndex,
    pub consts: BTreeMap<DefId, HirConstItem>,
    pub fns: BTreeMap<DefId, HirFnItem>,
    pub enums: BTreeMap<DefId, HirEnumItem>,
    pub enum_variants: BTreeMap<HirEnumVariantKey, HirEnumVariant>,
    pub structs: BTreeMap<DefId, HirStructItem>,
    pub bundles: BTreeMap<DefId, HirBundleItem>,
    pub interfaces: BTreeMap<DefId, HirInterfaceItem>,
    pub maps: BTreeMap<DefId, HirMapItem>,
    pub callables: BTreeMap<DefId, HirCallable>,
}

impl HirDesign {
    /// Creates an empty design with no packages, definitions, or expressions.
    pub fn empty() -> Self {
        Self {
            packages: Vec::new(),
            module_docs: BTreeMap::new(),
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
            extension_methods: HirExtensionMethodIndex::new(),
            consts: BTreeMap::new(),
            fns: BTreeMap::new(),
            enums: BTreeMap::new(),
            enum_variants: BTreeMap::new(),
            structs: BTreeMap::new(),
            bundles: BTreeMap::new(),
            interfaces: BTreeMap::new(),
            maps: BTreeMap::new(),
            callables: BTreeMap::new(),
        }
    }

    /// Returns the name of a definition by its ID.
    pub fn def_name(&self, id: DefId) -> Option<&str> {
        self.defs.get(id.get()).map(|def| def.name.as_str())
    }

    /// Returns all definition IDs visible from the given owner's package.
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

    /// Returns all definition IDs that belong to a given source file.
    pub fn source_def_ids(&self, source: syl_span::SourceId) -> Vec<DefId> {
        self.defs
            .iter()
            .filter(|def| def.span.source == source)
            .map(|def| def.id)
            .collect()
    }

    /// Returns the doc comment for a source module, if any.
    pub fn doc_for_module(&self, source: SourceId) -> Option<&str> {
        self.module_docs.get(&source).map(String::as_str)
    }

    /// Returns the doc comment for a definition item, searching across
    /// all item kinds (const, fn, enum, bundle, interface, map, cell).
    pub fn doc_for_item(&self, def: DefId) -> Option<&str> {
        self.consts
            .get(&def)
            .and_then(|item| item.doc.as_deref())
            .or_else(|| self.fns.get(&def).and_then(|item| item.doc.as_deref()))
            .or_else(|| self.enums.get(&def).and_then(|item| item.doc.as_deref()))
            .or_else(|| self.structs.get(&def).and_then(|item| item.doc.as_deref()))
            .or_else(|| self.bundles.get(&def).and_then(|item| item.doc.as_deref()))
            .or_else(|| {
                self.interfaces
                    .get(&def)
                    .and_then(|item| item.doc.as_deref())
            })
            .or_else(|| self.maps.get(&def).and_then(|item| item.doc.as_deref()))
            .or_else(|| {
                self.callables
                    .get(&def)
                    .and_then(|callable| match callable {
                        HirCallable::Cell(item) => item.doc.as_deref(),
                        HirCallable::Extern(item) => item.doc.as_deref(),
                    })
            })
    }

    /// Returns the doc comment for a specific field of a bundle or interface definition.
    pub fn doc_for_field(&self, def: DefId, field: &str) -> Option<&str> {
        self.bundles
            .get(&def)
            .and_then(|item| {
                item.fields
                    .iter()
                    .find(|decl| decl.name == field)
                    .and_then(|decl| decl.doc.as_deref())
            })
            .or_else(|| {
                self.structs.get(&def).and_then(|item| {
                    item.fields
                        .iter()
                        .find(|decl| decl.name == field)
                        .and_then(|decl| decl.doc.as_deref())
                })
            })
            .or_else(|| {
                self.interfaces.get(&def).and_then(|item| {
                    item.fields
                        .iter()
                        .find(|decl| decl.name == field)
                        .and_then(|decl| decl.doc.as_deref())
                })
            })
    }

    /// Finds the definition that an import statement at the given span resolves to.
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

    /// Finds the member declaration at a span within a given owner definition.
    pub fn member_field_def_at(&self, owner: DefId, span: Span) -> Option<&HirMemberDecl> {
        self.member_decls
            .iter()
            .filter(|member| member.owner == owner && contains_span(member.span, span))
            .min_by_key(|member| span_width(member.span))
    }

    /// Finds any member declaration whose span contains the given cursor position.
    pub fn member_decl_definition_at(&self, span: Span) -> Option<&HirMemberDecl> {
        self.member_decls
            .iter()
            .filter(|member| contains_span(member.span, span))
            .min_by_key(|member| span_width(member.span))
    }

    /// Finds a type reference belonging to the given owner that contains this span.
    pub fn type_ref_at(&self, owner: DefId, span: Span) -> Option<&HirTypeRef> {
        self.type_refs
            .iter()
            .filter(|type_ref| type_ref.owner == owner && contains_span(type_ref.span, span))
            .min_by_key(|type_ref| span_width(type_ref.span))
    }

    /// Returns the extension methods available for a given receiver type and method name.
    pub fn extension_methods_for(&self, receiver: DefId, name: &str) -> &[DefId] {
        self.extension_methods.methods_for(receiver, name)
    }

    /// Registers an extension method for a receiver type.
    pub fn register_extension_method(&mut self, receiver: DefId, name: String, method: DefId) {
        self.extension_methods.register(receiver, name, method);
    }

    /// Resolves the view member for a `ViewSelect` type reference at the given span.
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

    /// Resolves a type reference to its canonical definition ID.
    pub fn resolved_type_def_for_ref(&self, type_ref: &HirTypeRef) -> Option<DefId> {
        self.type_def_for_mir_type(type_ref.owner, &type_ref.ty)
    }

    /// Returns all member declarations for a given owner (for autocompletion).
    pub fn member_completion_fields_at(&self, owner: DefId, _span: Span) -> Vec<&HirMemberDecl> {
        self.member_decls
            .iter()
            .filter(|member| owner == member.owner)
            .collect()
    }

    /// Resolves a `MirTypeRef` to its canonical definition ID.
    pub fn type_def_for_mir_type(&self, _owner: DefId, ty: &MirTypeRef) -> Option<DefId> {
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

    /// Returns the package path containing the given definition.
    pub fn package_path_for_def(&self, owner: DefId) -> Option<HirPath> {
        self.defs
            .get(owner.get())
            .map(|def| def.canonical_path.parent())
    }

    /// If `expr` is a field access on an enum, returns the enum's definition ID
    /// and the variant name.
    pub fn enum_variant_expr<'a>(&self, expr: &'a HirBodyExpr) -> Option<(DefId, &'a str)> {
        let (base, variant) = match &expr.node {
            HirExprNode::Field { base, field } => (base.as_ref(), field.as_str()),
            _ => return None,
        };
        let enum_def = self.enum_variant_base_def(base)?;
        self.enum_variants
            .contains_key(&HirEnumVariantKey::new(enum_def, variant))
            .then_some((enum_def, variant))
    }

    fn enum_variant_base_def(&self, expr: &HirBodyExpr) -> Option<DefId> {
        let mut current = expr;
        loop {
            match &current.node {
                HirExprNode::Group(inner) => current = inner,
                HirExprNode::Ident(_) => break,
                _ => return None,
            }
        }
        let HirResolution::Def(def) = self.expr_resolutions.get(&current.id()).copied()? else {
            return None;
        };
        self.defs
            .get(def.get())
            .filter(|item| item.kind == HirDefKind::Enum)
            .map(|item| item.id)
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

/// Index mapping receiver type → method name → list of extension method defs.
///
/// Used during resolution to find extension methods for a given type.
#[derive(Clone, Default)]
#[non_exhaustive]
pub struct HirExtensionMethodIndex {
    methods: BTreeMap<DefId, BTreeMap<String, Vec<DefId>>>,
}

impl HirExtensionMethodIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns all extension methods with `name` that apply to `receiver`.
    pub fn methods_for(&self, receiver: DefId, name: &str) -> &[DefId] {
        self.methods
            .get(&receiver)
            .and_then(|methods| methods.get(name))
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Registers an extension method for the given receiver type.
    pub fn register(&mut self, receiver: DefId, name: String, method: DefId) {
        self.methods
            .entry(receiver)
            .or_default()
            .entry(name)
            .or_default()
            .push(method);
    }
}

/// A package in the HIR — a namespace containing definitions.
///
/// `path` is the segmented namespace path (e.g. `["std", "logic"]`).
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

/// A resolved import binding a short name path to its canonical package path.
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

/// A single named definition in the HIR.
///
/// Every item declaration (`const`, `fn`, `enum`, etc.) becomes a `HirDef`
/// with a unique `DefId`, a human-readable `name`, a `canonical_path` for
/// cross-referencing, and a `kind` that dispatches to the item's body.
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

/// What kind of declaration a `HirDef` represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, IntoStaticStr)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
pub enum HirDefKind {
    Const,
    Fn,
    Enum,
    Bundle,
    Struct,
    Interface,
    Map,
    Cell,
    #[strum(serialize = "extern cell")]
    ExternCell,
}

/// A local variable or binding within a definition body.
///
/// Locals cover parameters, let-bindings, variables, signals, registers,
/// loop variables, and instance names defined inside a function or cell.
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

/// The syntactic role of a `HirLocal`.
#[derive(Clone, Copy, IntoStaticStr)]
#[non_exhaustive]
#[strum(serialize_all = "snake_case")]
pub enum HirLocalKind {
    Generic,
    Param,
    Result,
    Const,
    Let,
    Var,
    Signal,
    Reg,
    Instance,
    Loop,
}

/// A single expression occurrence in the HIR, identified by its arena ID.
///
/// The expression's actual node data is stored in `HirDesign::exprs`
/// or in `HirBodyExpr` for body-local expressions.
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

/// A field access expression `base.field` in the HIR.
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

/// A declaration of a field, view, or view-field member within a type.
///
/// Member declarations associate field/view names with their owner type
/// and provide the information needed for name resolution and completion.
#[non_exhaustive]
pub struct HirMemberDecl {
    pub owner: DefId,
    pub doc: Option<String>,
    pub name: String,
    pub kind: HirMemberKind,
    pub span: Span,
}

impl HirMemberDecl {
    pub fn new(owner: DefId, name: String, kind: HirMemberKind, span: Span) -> Self {
        Self {
            owner,
            doc: None,
            name,
            kind,
            span,
        }
    }

    pub fn with_doc(
        owner: DefId,
        doc: Option<String>,
        name: String,
        kind: HirMemberKind,
        span: Span,
    ) -> Self {
        Self {
            owner,
            doc,
            name,
            kind,
            span,
        }
    }
}

/// The kind of a member declaration within a type.
///
/// `Field` — a data field with its type.
/// `View` — a named view (interface directional subgroup).
/// `ViewField` — a field within a named view.
#[derive(Clone, Debug, PartialEq, IntoStaticStr)]
#[non_exhaustive]
pub enum HirMemberKind {
    #[strum(serialize = "field")]
    Field { ty: MirTypeRef },
    #[strum(serialize = "view")]
    View,
    #[strum(serialize = "view field")]
    ViewField { view: String },
}

impl HirMemberKind {
    pub fn label(&self) -> &'static str {
        self.clone().into()
    }
}

/// A type annotation occurrence in the HIR, linking a type expression
/// to the definition body that owns it.
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
