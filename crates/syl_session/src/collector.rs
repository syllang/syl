use crate::ProjectError;
use std::{fs, path::Path, path::PathBuf};

#[non_exhaustive]
pub(crate) struct SylFileCollector<'a> {
    out: &'a mut Vec<PathBuf>,
}

impl<'a> SylFileCollector<'a> {
    pub(crate) fn new(out: &'a mut Vec<PathBuf>) -> Self {
        Self { out }
    }

    pub(crate) fn collect(&mut self, path: &Path) -> Result<(), ProjectError> {
        if path.is_file() {
            if path.extension().and_then(|ext| ext.to_str()) == Some("syl") {
                self.out.push(path.to_path_buf());
            }
            return Ok(());
        }
        let entries = fs::read_dir(path).map_err(|source| ProjectError::ReadDir {
            path: path.to_path_buf(),
            source,
        })?;
        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|source| ProjectError::ReadDirEntry {
                path: path.to_path_buf(),
                source,
            })?;
            paths.push(entry.path());
        }
        paths.sort();
        for child in paths {
            self.collect(&child)?;
        }
        Ok(())
    }
}
