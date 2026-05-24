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
    Module,
    ExternModule,
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
            HirDefKind::Module => Self::Module,
            HirDefKind::ExternModule => Self::ExternModule,
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

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ImportEdge {
    path: HirPath,
    target: Option<DefId>,
    span: Span,
}

impl ImportEdge {
    fn new(path: HirPath, target: Option<DefId>, span: Span) -> Self {
        Self { path, target, span }
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
pub struct PackageSummary {
    package_id: Option<PackageId>,
    path: HirPath,
    definitions: Vec<DefId>,
    imports: Vec<ImportEdge>,
}

impl PackageSummary {
    fn new(package_id: Option<PackageId>, path: HirPath) -> Self {
        Self {
            package_id,
            path,
            definitions: Vec::new(),
            imports: Vec::new(),
        }
    }

    pub fn package_id(&self) -> Option<PackageId> {
        self.package_id
    }

    pub fn path(&self) -> &HirPath {
        &self.path
    }

    pub fn definitions(&self) -> &[DefId] {
        &self.definitions
    }

    pub fn imports(&self) -> &[ImportEdge] {
        &self.imports
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DefinitionPath {
    def: DefId,
    name: String,
    kind: DefinitionKind,
    canonical_path: HirPath,
    span: Span,
}

impl DefinitionPath {
    fn new(
        def: DefId,
        name: String,
        kind: DefinitionKind,
        canonical_path: HirPath,
        span: Span,
    ) -> Self {
        Self {
            def,
            name,
            kind,
            canonical_path,
            span,
        }
    }

    pub fn def(&self) -> DefId {
        self.def
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
    definitions: BTreeMap<DefId, DefinitionPath>,
}

impl ResolutionGraph {
    fn collect(hir: &HirDesign) -> Self {
        let mut packages = BTreeMap::new();
        for package in &hir.packages {
            let path = HirPath::new(package.path.clone());
            packages.insert(path.clone(), PackageSummary::new(Some(package.id), path));
        }
        for def in &hir.defs {
            let package_path = def.canonical_path.parent();
            packages
                .entry(package_path.clone())
                .or_insert_with(|| PackageSummary::new(None, package_path))
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
                .or_insert_with(|| PackageSummary::new(None, import.package_path.clone()))
                .imports
                .push(ImportEdge::new(
                    HirPath::new(import.path.clone()),
                    target,
                    import.span,
                ));
        }
        let definitions = hir
            .defs
            .iter()
            .map(|def| {
                (
                    def.id,
                    DefinitionPath::new(
                        def.id,
                        def.name.clone(),
                        DefinitionKind::from(def.kind),
                        def.canonical_path.clone(),
                        def.span,
                    ),
                )
            })
            .collect();
        Self {
            packages: packages.into_values().collect(),
            definitions,
        }
    }

    pub fn packages(&self) -> &[PackageSummary] {
        &self.packages
    }

    pub fn definition(&self, def: DefId) -> Option<&DefinitionPath> {
        self.definitions.get(&def)
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
                definitions: BTreeMap::new(),
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
