use std::path::PathBuf;

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ProjectConfig {
    workspace_roots: Vec<PathBuf>,
    std_roots: Vec<PathBuf>,
    package_roots: Vec<PathBuf>,
}

impl ProjectConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn builder() -> ProjectConfigBuilder {
        ProjectConfigBuilder::new()
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

#[derive(Clone, Debug, Default)]
#[non_exhaustive]
pub struct ProjectConfigBuilder {
    workspace_roots: Vec<PathBuf>,
    std_roots: Vec<PathBuf>,
    package_roots: Vec<PathBuf>,
}

impl ProjectConfigBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn workspace_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.workspace_roots = roots;
        self
    }

    pub fn std_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.std_roots = roots;
        self
    }

    pub fn package_roots(mut self, roots: Vec<PathBuf>) -> Self {
        self.package_roots = roots;
        self
    }

    pub fn push_workspace_root(mut self, root: PathBuf) -> Self {
        self.workspace_roots.push(root);
        self
    }

    pub fn push_std_root(mut self, root: PathBuf) -> Self {
        self.std_roots.push(root);
        self
    }

    pub fn push_package_root(mut self, root: PathBuf) -> Self {
        self.package_roots.push(root);
        self
    }

    pub fn build(self) -> ProjectConfig {
        ProjectConfig {
            workspace_roots: self.workspace_roots,
            std_roots: self.std_roots,
            package_roots: self.package_roots,
        }
    }
}

impl From<ProjectConfigBuilder> for ProjectConfig {
    fn from(value: ProjectConfigBuilder) -> Self {
        value.build()
    }
}
