use crate::hir::{HirDefKind, HirDesign};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId, HirPath, HirResolution, LocalId, PackageId};
use syl_span::Span;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DefinitionKind {
    Const,
    Fn,
    Enum,
    Bundle,
    Interface,
    Map,
    Cell,
    ExternCell,
}

impl From<HirDefKind> for DefinitionKind {
    fn from(value: HirDefKind) -> Self {
        match value {
            HirDefKind::Const => Self::Const,
            HirDefKind::Fn => Self::Fn,
            HirDefKind::Enum => Self::Enum,
            HirDefKind::Bundle => Self::Bundle,
            HirDefKind::Interface => Self::Interface,
            HirDefKind::Map => Self::Map,
            HirDefKind::Cell => Self::Cell,
            HirDefKind::ExternCell => Self::ExternCell,
            _ => unreachable!("unsupported HIR definition kind in semantic facts"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum HirFactId {
    Def(DefId),
    Local(LocalId),
    Expr(ExprId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SemanticResolution {
    Def(DefId),
    Local(LocalId),
}

impl From<HirResolution> for SemanticResolution {
    fn from(value: HirResolution) -> Self {
        match value {
            HirResolution::Def(def) => Self::Def(def),
            HirResolution::Local(local) => Self::Local(local),
            _ => unreachable!("unsupported HIR resolution in semantic facts"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct PackageNodeId {
    index: usize,
}

impl PackageNodeId {
    fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn get(self) -> usize {
        self.index
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct ImportId {
    index: usize,
}

impl ImportId {
    fn new(index: usize) -> Self {
        Self { index }
    }

    pub fn get(self) -> usize {
        self.index
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct PackageSummary {
    id: PackageNodeId,
    package_id: Option<PackageId>,
    path: HirPath,
}

impl PackageSummary {
    fn new(id: PackageNodeId, package_id: Option<PackageId>, path: HirPath) -> Self {
        Self {
            id,
            package_id,
            path,
        }
    }

    pub fn id(&self) -> PackageNodeId {
        self.id
    }

    pub fn package_id(&self) -> Option<PackageId> {
        self.package_id
    }

    pub fn path(&self) -> &HirPath {
        &self.path
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImportEdge {
    id: ImportId,
    package: PackageNodeId,
    path: HirPath,
    target: Option<DefId>,
    span: Span,
}

impl ImportEdge {
    fn new(
        id: ImportId,
        package: PackageNodeId,
        path: HirPath,
        target: Option<DefId>,
        span: Span,
    ) -> Self {
        Self {
            id,
            package,
            path,
            target,
            span,
        }
    }

    pub fn id(&self) -> ImportId {
        self.id
    }

    pub fn package(&self) -> PackageNodeId {
        self.package
    }

    pub fn path(&self) -> &HirPath {
        &self.path
    }

    pub fn target(&self) -> Option<DefId> {
        self.target
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DefinitionPath {
    def: DefId,
    package: PackageNodeId,
    name: String,
    kind: DefinitionKind,
    canonical_path: HirPath,
    span: Span,
}

struct DefinitionPathInput {
    def: DefId,
    package: PackageNodeId,
    name: String,
    kind: DefinitionKind,
    canonical_path: HirPath,
    span: Span,
}

impl DefinitionPath {
    fn new(input: DefinitionPathInput) -> Self {
        Self {
            def: input.def,
            package: input.package,
            name: input.name,
            kind: input.kind,
            canonical_path: input.canonical_path,
            span: input.span,
        }
    }

    pub fn def(&self) -> DefId {
        self.def
    }

    pub fn package(&self) -> PackageNodeId {
        self.package
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn kind(&self) -> DefinitionKind {
        self.kind
    }

    pub fn canonical_path(&self) -> &HirPath {
        &self.canonical_path
    }

    pub fn span(&self) -> Span {
        self.span
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolutionGraph {
    packages: Vec<PackageSummary>,
    imports: Vec<ImportEdge>,
    definitions: BTreeMap<DefId, DefinitionPath>,
    modules: Vec<DefId>,
    package_definitions: BTreeMap<PackageNodeId, Vec<DefId>>,
    package_modules: BTreeMap<PackageNodeId, Vec<DefId>>,
    package_imports: BTreeMap<PackageNodeId, Vec<ImportId>>,
}

impl ResolutionGraph {
    fn collect(hir: &HirDesign) -> Self {
        let mut packages = BTreeMap::new();
        for package in &hir.packages {
            let path = HirPath::new(package.path.clone());
            packages.insert(path.clone(), PackageFacts::new(Some(package.id)));
        }
        for def in &hir.defs {
            let package_path = def.canonical_path.parent();
            packages
                .entry(package_path.clone())
                .or_insert_with(|| PackageFacts::new(None))
                .definitions
                .push(def.id);
        }
        for import in &hir.imports {
            let target = hir
                .canonical_def_names
                .get(&HirPath::new(import.path.clone()))
                .copied();
            packages
                .entry(import.package_path.clone())
                .or_insert_with(|| PackageFacts::new(None))
                .imports
                .push(PendingImport::new(
                    HirPath::new(import.path.clone()),
                    target,
                    import.span,
                ));
        }

        let package_ids = packages
            .keys()
            .cloned()
            .enumerate()
            .map(|(index, path)| (path, PackageNodeId::new(index)))
            .collect::<BTreeMap<_, _>>();
        let mut package_nodes = Vec::new();
        let mut import_nodes = Vec::new();
        let mut definitions = BTreeMap::new();
        let mut modules = Vec::new();
        let mut package_definitions = BTreeMap::new();
        let mut package_modules = BTreeMap::new();
        let mut package_imports = BTreeMap::new();

        for (path, facts) in packages {
            let package = package_ids
                .get(&path)
                .copied()
                .expect("package ids must exist for every collected path");
            package_nodes.push(PackageSummary::new(package, facts.package_id, path));

            for def in facts.definitions {
                let Some(hir_def) = hir.defs.get(def.get()) else {
                    continue;
                };
                definitions.insert(
                    def,
                    DefinitionPath::new(DefinitionPathInput {
                        def,
                        package,
                        name: hir_def.name.clone(),
                        kind: DefinitionKind::from(hir_def.kind),
                        canonical_path: hir_def.canonical_path.clone(),
                        span: hir_def.span,
                    }),
                );
                package_definitions
                    .entry(package)
                    .or_insert_with(Vec::new)
                    .push(def);
                if matches!(hir_def.kind, HirDefKind::Cell | HirDefKind::ExternCell) {
                    modules.push(def);
                    package_modules
                        .entry(package)
                        .or_insert_with(Vec::new)
                        .push(def);
                }
            }

            for import in facts.imports {
                let id = ImportId::new(import_nodes.len());
                import_nodes.push(ImportEdge::new(
                    id,
                    package,
                    import.path,
                    import.target,
                    import.span,
                ));
                package_imports
                    .entry(package)
                    .or_insert_with(Vec::new)
                    .push(id);
            }
        }

        Self {
            packages: package_nodes,
            imports: import_nodes,
            definitions,
            modules,
            package_definitions,
            package_modules,
            package_imports,
        }
    }

    pub fn packages(&self) -> &[PackageSummary] {
        &self.packages
    }

    pub fn modules(&self) -> &[DefId] {
        &self.modules
    }

    pub fn imports(&self) -> &[ImportEdge] {
        &self.imports
    }

    pub fn package(&self, id: PackageNodeId) -> Option<&PackageSummary> {
        self.packages.get(id.get())
    }

    pub fn package_definitions(&self, package: PackageNodeId) -> &[DefId] {
        self.package_definitions
            .get(&package)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn package_modules(&self, package: PackageNodeId) -> &[DefId] {
        self.package_modules
            .get(&package)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn package_imports(&self, package: PackageNodeId) -> &[ImportId] {
        self.package_imports
            .get(&package)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    pub fn definition(&self, def: DefId) -> Option<&DefinitionPath> {
        self.definition_path(def)
    }

    pub fn definition_path(&self, def: DefId) -> Option<&DefinitionPath> {
        self.definitions.get(&def)
    }

    pub fn definitions(&self) -> impl Iterator<Item = &DefinitionPath> {
        self.definitions.values()
    }

    pub fn import(&self, id: ImportId) -> Option<&ImportEdge> {
        self.imports.get(id.get())
    }

    pub fn import_target(&self, id: ImportId) -> Option<DefId> {
        self.import(id).and_then(ImportEdge::target)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResolutionTable {
    graph: ResolutionGraph,
    values: BTreeMap<HirFactId, SemanticResolution>,
}

impl ResolutionTable {
    pub(crate) fn empty() -> Self {
        Self {
            graph: ResolutionGraph {
                packages: Vec::new(),
                imports: Vec::new(),
                definitions: BTreeMap::new(),
                modules: Vec::new(),
                package_definitions: BTreeMap::new(),
                package_modules: BTreeMap::new(),
                package_imports: BTreeMap::new(),
            },
            values: BTreeMap::new(),
        }
    }

    pub(crate) fn collect(hir: &HirDesign) -> Self {
        let values = hir
            .expr_resolutions
            .iter()
            .map(|(expr, resolution)| {
                (
                    HirFactId::Expr(*expr),
                    SemanticResolution::from(*resolution),
                )
            })
            .collect();
        Self {
            graph: ResolutionGraph::collect(hir),
            values,
        }
    }

    pub fn graph(&self) -> &ResolutionGraph {
        &self.graph
    }

    pub fn get(&self, id: HirFactId) -> Option<SemanticResolution> {
        self.values.get(&id).copied()
    }
}

struct PackageFacts {
    package_id: Option<PackageId>,
    definitions: Vec<DefId>,
    imports: Vec<PendingImport>,
}

impl PackageFacts {
    fn new(package_id: Option<PackageId>) -> Self {
        Self {
            package_id,
            definitions: Vec::new(),
            imports: Vec::new(),
        }
    }
}

struct PendingImport {
    path: HirPath,
    target: Option<DefId>,
    span: Span,
}

impl PendingImport {
    fn new(path: HirPath, target: Option<DefId>, span: Span) -> Self {
        Self { path, target, span }
    }
}
