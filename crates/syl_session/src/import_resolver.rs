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
        let source_module = parts.get(..parts.len().checked_sub(1)?)?;
        for base in self.config.workspace_roots() {
            if let Some(path) = self.resolve_candidate(base, source_module, &mut overlay_exists) {
                return Some(path);
            }
        }
        for base in self.config.std_roots() {
            let Some(candidate_parts) = source_module.strip_prefix(&["std".to_string()]) else {
                continue;
            };
            if let Some(path) = self.resolve_candidate(base, candidate_parts, &mut overlay_exists) {
                return Some(path);
            }
        }
        for base in self.config.package_roots() {
            if let Some(path) = self.resolve_candidate(base, source_module, &mut overlay_exists) {
                return Some(path);
            }
        }
        None
    }

    fn use_candidate(&self, parts: &[String]) -> Option<PathBuf> {
        (!parts.is_empty()).then(|| PathBuf::from(format!("{}.syl", parts.join("/"))))
    }

    fn resolve_candidate<F>(
        &self,
        base: &Path,
        parts: &[String],
        overlay_exists: &mut F,
    ) -> Option<PathBuf>
    where
        F: FnMut(&Path) -> bool,
    {
        let path = base.join(self.use_candidate(parts)?);
        (self.vfs.exists(&path) || overlay_exists(&path)).then_some(path)
    }
}
