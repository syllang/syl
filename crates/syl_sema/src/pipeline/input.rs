use syl_syntax::AstFile;

#[derive(Debug)]
#[non_exhaustive]
pub struct SemanticSourceFile<'files> {
    /// Dot-separated module hierarchy identifier (e.g., `["std", "logic"]` for `std.logic`,
    /// `["foo", "bar", "baz"]` for `foo.bar.baz`).
    ///
    /// This is **not** a filesystem path. It is constructed from the file's path relative to
    /// a source root, but after construction it lives purely as a namespace identifier:
    /// used for import resolution, canonical-definition lookup (`BTreeMap<HirPath, DefId>`),
    /// and display (joined with `"."`). The `.syl` extension is stripped; segments are
    /// validated as `[_a-zA-Z][_a-zA-Z0-9]*` identifiers.
    ///
    /// # Examples
    ///
    /// | File path                           | `module_path`            |
    /// |-------------------------------------|--------------------------|
    /// | `src/lib.syl`                       | `["lib"]`                |
    /// | `src/std/logic.syl`                 | `["std", "logic"]`       |
    /// | `vendor/pkg/foo/bar/baz.syl`        | `["foo", "bar", "baz"]`  |
    /// | (std root) `std/core.syl`           | `["std", "core"]`        |
    module_path: Vec<String>,
    ast: &'files AstFile,
}

impl<'files> SemanticSourceFile<'files> {
    pub fn new(module_path: Vec<String>, ast: &'files AstFile) -> Self {
        Self { module_path, ast }
    }

    pub fn module_path(&self) -> &[String] {
        &self.module_path
    }

    pub fn ast(&self) -> &'files AstFile {
        self.ast
    }
}
