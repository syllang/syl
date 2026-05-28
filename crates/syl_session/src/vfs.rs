use crate::ProjectError;
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

/// Abstract virtual file system — allows plugging in a real FS or a test VFS.
pub trait Vfs: fmt::Debug {
    fn read_to_string(&self, path: &Path) -> Result<String, ProjectError>;

    fn exists(&self, path: &Path) -> bool;

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, ProjectError>;
}

/// The real filesystem Vfs implementation (delegates to `std::fs`).
#[derive(Clone, Copy, Debug, Default)]
#[non_exhaustive]
pub struct FsVfs;

impl Vfs for FsVfs {
    fn read_to_string(&self, path: &Path) -> Result<String, ProjectError> {
        fs::read_to_string(path).map_err(|source| ProjectError::Read {
            path: path.to_path_buf(),
            source,
        })
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, ProjectError> {
        path.canonicalize()
            .map_err(|source| ProjectError::Canonicalize {
                path: path.to_path_buf(),
                source,
            })
    }
}
