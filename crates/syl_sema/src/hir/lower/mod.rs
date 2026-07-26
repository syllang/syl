use crate::{CompileError, SemanticSourceFile, hir::HirDesign, hir::resolve::HirNameResolver};
use syl_hir::name::HirPath;

mod index;
mod insert;

/// AST → HIR lowering and name-index construction.
///
/// Owns a batch of [`SemanticSourceFile`]s and an in-progress [`HirDesign`].
/// Prefer the higher-level [`SemanticSession`](crate::SemanticSession) for the
/// full semantic pipeline; use `HirResolver` when only a raw [`HirDesign`] is
/// needed (tests, internal stages, or custom wrapping into
/// [`HirAnalysis`](crate::HirAnalysis)).
///
/// Implementation is split by concern:
///
/// ```text
///  lower/mod.rs     HirResolver API, build_index orchestration
///  lower/insert.rs  package/import/item insert + register_* locals
///  lower/index.rs   expression/type indexing (ExprId arena)
/// ```
///
/// # Main usage flow
///
/// ```text
///  AstFile[] / SemanticSourceFile[]
///              |
///              v
///    +---------------------+
///    |     HirResolver     |   new(&[AstFile])
///    |  sources + design   |   new_sources(sources)
///    +----------+----------+
///               |
///               |  resolve* (consumes self)
///               v
///     +---------+----------+
///     | 1. build_index     |  packages, imports, item defs,
///     |    insert + index  |  locals/exprs skeleton, extension methods
///     +---------+----------+
///               |
///               v
///     +---------+----------+
///     | 2. HirNameResolver |  resolve names into resolutions
///     +---------+----------+
///               |
///       +-------+---------------------------+
///       |                                   |
///       v                                   v
///  resolve()                      resolve_partial()
///  Result<HirDesign, E>           (HirDesign, Vec<E>)
///  (fail-fast)                    (always returns design)
///
///  resolve_collect()
///  Result<HirDesign, Vec<E>>      (Ok only if zero errors)
///
///  Downstream:
///    HirDesign  -->  HirAnalysis::new  -->  check_tir / IDE queries
///    (via SemanticSession::resolve_hir[ _partial ])
/// ```
///
/// Typical paths:
/// - **Strict**: `HirResolver::new(files).resolve()?`
/// - **IDE / partial**: `resolve_partial()` then wrap the design even if
///   errors are non-empty
/// - **Session façade**: `SemanticSession` calls these APIs for you
#[non_exhaustive]
pub struct HirResolver<'files> {
    sources: Vec<SemanticSourceFile<'files>>,
    design: HirDesign,
}

impl<'files> HirResolver<'files> {
    /// Build a resolver from AST files, assigning default module paths
    /// `file0`, `file1`, ...
    pub fn new(files: &'files [syl_syntax::AstFile]) -> Self {
        let sources = files
            .iter()
            .enumerate()
            .map(|(index, ast)| SemanticSourceFile::new(vec![format!("file{index}")], ast))
            .collect();
        Self::new_sources(sources)
    }

    /// Build a resolver from sources that already carry module paths.
    pub fn new_sources(sources: Vec<SemanticSourceFile<'files>>) -> Self {
        Self {
            sources,
            design: HirDesign::empty(),
        }
    }

    /// Index all sources and resolve names, failing on the first error.
    pub fn resolve(mut self) -> Result<HirDesign, CompileError> {
        self.build_index()?;
        HirNameResolver::new(&mut self.design).resolve()?;
        Ok(self.design)
    }

    /// Index and resolve, collecting errors; returns `Ok` only if none remain.
    pub fn resolve_collect(mut self) -> Result<HirDesign, Vec<CompileError>> {
        let mut errors = self.build_index_collect();
        if let Err(mut resolve_errors) =
            HirNameResolver::new_collect(&mut self.design).resolve_collect()
        {
            errors.append(&mut resolve_errors);
        }
        if errors.is_empty() {
            Ok(self.design)
        } else {
            Err(errors)
        }
    }

    /// Index and resolve while always returning the best-effort [`HirDesign`].
    ///
    /// Prefer this for IDE / recovery paths that still need a partial graph.
    pub fn resolve_partial(mut self) -> (HirDesign, Vec<CompileError>) {
        let mut errors = self.build_index_collect();
        if let Err(mut resolve_errors) =
            HirNameResolver::new_collect(&mut self.design).resolve_collect()
        {
            errors.append(&mut resolve_errors);
        }
        (self.design, errors)
    }

    fn build_index(&mut self) -> Result<(), CompileError> {
        let sources = std::mem::take(&mut self.sources);
        for source in sources {
            let package = PackageScope::new(source.module_path());
            self.insert_package(&source);
            self.insert_imports(&source, &package);
            for item in &source.ast().items {
                self.insert_item(item, &package)?;
            }
        }
        self.validate_imports()?;
        self.register_extension_methods();
        Ok(())
    }

    fn build_index_collect(&mut self) -> Vec<CompileError> {
        let mut errors = Vec::new();
        let sources = std::mem::take(&mut self.sources);
        for source in sources {
            let package = PackageScope::new(source.module_path());
            self.insert_package(&source);
            self.insert_imports(&source, &package);
            for item in &source.ast().items {
                if let Err(error) = self.insert_item(item, &package) {
                    errors.push(error);
                }
            }
        }
        errors.extend(self.validate_imports_collect());
        self.register_extension_methods();
        errors
    }
}

/// Logical package path for a source file during HIR indexing.
#[non_exhaustive]
pub(super) struct PackageScope {
    pub(super) path: HirPath,
}

impl PackageScope {
    pub(super) fn new(module_path: &[String]) -> Self {
        let path = HirPath::new(module_path.to_vec());
        Self { path }
    }

    pub(super) fn canonical_def_path(&self, name: &str) -> HirPath {
        self.path.with_leaf(name)
    }
}
