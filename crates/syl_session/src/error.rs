use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ProjectError {
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to read directory {path}: {source}")]
    ReadDir {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to read entry in {path}: {source}")]
    ReadDirEntry {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to canonicalize {path}: {source}")]
    Canonicalize {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("document is not open: {uri}")]
    DocumentNotOpen { uri: String },
    #[error("stale document version for {uri}: requested {requested}, current {current}")]
    StaleDocumentVersion {
        uri: String,
        requested: u64,
        current: u64,
    },
}
