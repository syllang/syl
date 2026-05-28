use derive_builder::Builder;
use std::path::PathBuf;

/// Configuration for a Syl project — sets workspace, standard library,
/// and package search roots.
#[derive(Clone, Debug, Default, Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build"))]
#[non_exhaustive]
pub struct ProjectConfig {
    #[builder(default)]
    workspace_roots: Vec<PathBuf>,
    #[builder(default)]
    std_roots: Vec<PathBuf>,
    #[builder(default)]
    package_roots: Vec<PathBuf>,
}

impl ProjectConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> ProjectConfigBuilder {
        ProjectConfigBuilder::default()
    }

    pub fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_roots
    }

    pub fn std_roots(&self) -> &[PathBuf] {
        &self.std_roots
    }

    pub fn package_roots(&self) -> &[PathBuf] {
        &self.package_roots
    }

    pub fn with_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_roots.push(root);
        self
    }

    pub fn with_std_root(mut self, root: PathBuf) -> Self {
        self.std_roots.push(root);
        self
    }

    pub fn with_package_root(mut self, root: PathBuf) -> Self {
        self.package_roots.push(root);
        self
    }
}

impl ProjectConfigBuilder {
    pub fn push_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_roots.get_or_insert_with(Vec::new).push(root);
        self
    }

    pub fn push_std_root(mut self, root: PathBuf) -> Self {
        self.std_roots.get_or_insert_with(Vec::new).push(root);
        self
    }

    pub fn push_package_root(mut self, root: PathBuf) -> Self {
        self.package_roots.get_or_insert_with(Vec::new).push(root);
        self
    }

    pub fn build(self) -> ProjectConfig {
        self.try_build().expect(
            "ProjectConfigBuilder only fails when required roots are missing, which cannot happen",
        )
    }
}
