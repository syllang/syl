use crate::{
    config::ProjectConfig,
    vfs::{FsVfs, Vfs},
};
use std::path::{Path, PathBuf};

#[derive(Debug)]
#[non_exhaustive]
pub struct ImportResolver<V: Vfs = FsVfs> {
    config: ProjectConfig,
    vfs: V,
}

impl ImportResolver<FsVfs> {
    pub fn new(config: ProjectConfig) -> Self {
        Self::with_vfs(config, FsVfs)
    }
}

impl<V: Vfs> ImportResolver<V> {
    pub fn with_vfs(config: ProjectConfig, vfs: V) -> Self {
        Self { config, vfs }
    }

    pub fn config(&self) -> &ProjectConfig {
        &self.config
    }

    pub fn vfs(&self) -> &V {
        &self.vfs
    }

    pub fn resolve_use<F>(&self, parts: &[String], mut overlay_exists: F) -> Option<PathBuf>
    where
        F: FnMut(&Path) -> bool,
    {
        let candidates = self.use_candidates(parts);
        for base in self.import_bases() {
            for candidate in &candidates {
                let path = base.join(candidate);
                if self.vfs.exists(&path) || overlay_exists(&path) {
                    return Some(path);
                }
            }
        }
        None
    }

    fn import_bases(&self) -> impl Iterator<Item = &Path> {
        self.config
            .workspace_roots()
            .iter()
            .map(|root| root.as_path())
            .chain(self.config.std_roots().iter().map(|root| root.as_path()))
            .chain(
                self.config
                    .package_roots()
                    .iter()
                    .map(|root| root.as_path()),
            )
    }

    fn use_candidates(&self, parts: &[String]) -> Vec<PathBuf> {
        let mut candidates = Vec::new();
        if parts.len() > 1 {
            candidates.push(PathBuf::from(format!(
                "{}.syl",
                parts[..parts.len() - 1].join("/")
            )));
        }
        if !parts.is_empty() {
            candidates.push(PathBuf::from(format!("{}.syl", parts.join("/"))));
        }
        candidates
    }
}
