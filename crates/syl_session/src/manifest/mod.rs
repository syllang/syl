//! Project configuration and external metadata decoding at the VFS boundary.
mod opaque;
#[cfg(test)]
mod opaque_tests;
#[cfg(test)]
mod tests;

use crate::{FsVfs, ProjectConfig, ProjectError, Vfs};
use serde::Deserialize;
use std::path::{Component, Path, PathBuf};
use syl_sema::OpaqueSummaryTable;

/// Conventional project manifest filename.
pub const MANIFEST_FILE_NAME: &str = "syl.toml";

/// Parsed and path-resolved `syl.toml` project manifest.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ProjectManifest {
    manifest_path: PathBuf,
    root_dir: PathBuf,
    name: Option<String>,
    version: Option<String>,
    edition: Option<String>,
    workspace_roots: Vec<PathBuf>,
    std_roots: Vec<PathBuf>,
    package_roots: Vec<PathBuf>,
    opaque_summary_paths: Vec<PathBuf>,
}

impl ProjectManifest {
    /// Conventional project manifest filename.
    pub const FILE_NAME: &str = MANIFEST_FILE_NAME;

    /// Discover the nearest manifest starting at a file or directory.
    pub fn discover(start: &Path) -> Result<Option<PathBuf>, ProjectError> {
        Self::discover_with_vfs(start, &FsVfs)
    }

    /// Discover the nearest manifest using an explicit virtual filesystem.
    pub fn discover_with_vfs<V: Vfs>(
        start: &Path,
        vfs: &V,
    ) -> Result<Option<PathBuf>, ProjectError> {
        let mut anchor = absolute(start);
        if vfs.exists(&anchor) {
            anchor = vfs.canonicalize(&anchor)?;
        }
        if anchor.file_name() == Some(MANIFEST_FILE_NAME.as_ref()) && vfs.exists(&anchor) {
            return Ok(Some(anchor));
        }
        for dir in anchor.ancestors() {
            let candidate = dir.join(MANIFEST_FILE_NAME);
            if vfs.exists(&candidate) {
                return vfs.canonicalize(&candidate).map(Some);
            }
        }
        Ok(None)
    }

    /// Load and validate a manifest from disk.
    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        Self::load_with_vfs(path, &FsVfs)
    }

    /// Load, validate, and resolve paths using an explicit virtual filesystem.
    pub fn load_with_vfs<V: Vfs>(path: &Path, vfs: &V) -> Result<Self, ProjectError> {
        let mut path = absolute(path);
        if vfs.exists(&path) {
            path = vfs.canonicalize(&path)?;
        }
        let invalid = |message: String| ProjectError::ManifestInvalid {
            path: path.clone(),
            message,
        };
        let root = path
            .parent()
            .ok_or_else(|| invalid("manifest has no parent directory".into()))?;
        let raw: RawManifest = toml::from_str(&vfs.read_to_string(&path)?).map_err(|err| {
            ProjectError::ManifestParse {
                path: path.clone(),
                message: err.to_string(),
            }
        })?;
        if raw
            .project
            .name
            .as_ref()
            .is_some_and(|name| name.trim().is_empty())
        {
            return Err(invalid("project.name must not be blank".into()));
        }
        let resolve = |paths: Vec<String>| -> Result<Vec<PathBuf>, ProjectError> {
            let mut resolved = Vec::new();
            for raw in paths {
                if raw.trim().is_empty() {
                    return Err(invalid("configured paths must not be blank".into()));
                }
                let joined = root.join(raw);
                let path = if vfs.exists(&joined) {
                    vfs.canonicalize(&joined)?
                } else {
                    normalize_lexical(&joined)
                };
                if !resolved.contains(&path) {
                    resolved.push(path);
                }
            }
            Ok(resolved)
        };
        Ok(Self {
            manifest_path: path.clone(),
            root_dir: root.to_path_buf(),
            name: raw.project.name,
            version: raw.project.version,
            edition: raw.project.edition,
            workspace_roots: resolve(
                raw.paths
                    .workspace_roots
                    .unwrap_or_else(|| vec![".".into()]),
            )?,
            std_roots: resolve(raw.paths.std_roots)?,
            package_roots: resolve(raw.paths.package_roots)?,
            opaque_summary_paths: resolve(raw.paths.opaque_summaries)?,
        })
    }

    /// Discover and load the nearest manifest, if any.
    pub fn discover_and_load(start: &Path) -> Result<Option<Self>, ProjectError> {
        Self::discover_and_load_with_vfs(start, &FsVfs)
    }

    /// Discover and load using an explicit virtual filesystem.
    pub fn discover_and_load_with_vfs<V: Vfs>(
        start: &Path,
        vfs: &V,
    ) -> Result<Option<Self>, ProjectError> {
        Self::discover_with_vfs(start, vfs)?
            .map(|path| Self::load_with_vfs(&path, vfs))
            .transpose()
    }

    pub fn manifest_path(&self) -> &Path {
        &self.manifest_path
    }
    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }
    pub fn version(&self) -> Option<&str> {
        self.version.as_deref()
    }
    pub fn edition(&self) -> Option<&str> {
        self.edition.as_deref()
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
    pub fn opaque_summary_paths(&self) -> &[PathBuf] {
        &self.opaque_summary_paths
    }

    /// Build a configuration containing the resolved search roots.
    pub fn to_project_config(&self) -> ProjectConfig {
        ProjectConfig::builder()
            .workspace_roots(self.workspace_roots.clone())
            .std_roots(self.std_roots.clone())
            .package_roots(self.package_roots.clone())
            .build()
    }

    /// Parse and merge all declared summary files in declaration order.
    pub fn load_opaque_summaries(&self) -> Result<OpaqueSummaryTable, ProjectError> {
        self.load_opaque_summaries_with_vfs(&FsVfs)
    }

    /// Parse and merge all declared summary files using an explicit VFS.
    pub fn load_opaque_summaries_with_vfs<V: Vfs>(
        &self,
        vfs: &V,
    ) -> Result<OpaqueSummaryTable, ProjectError> {
        let mut table = OpaqueSummaryTable::new();
        for path in &self.opaque_summary_paths {
            table.merge_from(&opaque::load(path, vfs)?);
        }
        Ok(table)
    }
}

fn absolute(path: &Path) -> PathBuf {
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir().map_or_else(|_| path.to_path_buf(), |cwd| cwd.join(path))
    }
}

fn normalize_lexical(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                if matches!(result.components().next_back(), Some(Component::Normal(_))) {
                    result.pop();
                } else if !result.has_root() {
                    result.push(component);
                }
            }
            _ => result.push(component),
        }
    }
    result
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawManifest {
    project: RawProject,
    paths: RawPaths,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawProject {
    name: Option<String>,
    version: Option<String>,
    edition: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct RawPaths {
    workspace_roots: Option<Vec<String>>,
    std_roots: Vec<String>,
    package_roots: Vec<String>,
    opaque_summaries: Vec<String>,
}
