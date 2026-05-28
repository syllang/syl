use crate::DocumentUri;
use std::path::{Path, PathBuf};

/// A document version identifier — monotonic numeric counter.
///
/// Versions are compared and hashed by their numeric value only.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct DocumentVersion {
    value: u64,
}

impl DocumentVersion {
    pub fn new(value: u64) -> Self {
        Self { value }
    }

    pub fn zero() -> Self {
        Self::new(0)
    }

    pub fn get(self) -> u64 {
        self.value
    }

    pub fn next(self) -> Self {
        Self::new(self.value.saturating_add(1))
    }
}

impl Default for DocumentVersion {
    fn default() -> Self {
        Self::zero()
    }
}

/// Where a document was loaded from — the real filesystem or an in-memory overlay.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DocumentOrigin {
    Disk,
    Overlay,
}

/// A source document loaded into the session — tracks URI, version, text, and origin.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceDocument {
    uri: DocumentUri,
    path: Option<PathBuf>,
    version: DocumentVersion,
    text: String,
    origin: DocumentOrigin,
}

impl SourceDocument {
    pub fn from_disk(path: PathBuf, text: String) -> Self {
        let uri = DocumentUri::from_file_path(&path);
        Self {
            uri,
            path: Some(path),
            version: DocumentVersion::zero(),
            text,
            origin: DocumentOrigin::Disk,
        }
    }

    pub fn from_overlay(
        uri: DocumentUri,
        text: String,
        version: DocumentVersion,
        path: Option<PathBuf>,
    ) -> Self {
        Self {
            uri,
            path,
            version,
            text,
            origin: DocumentOrigin::Overlay,
        }
    }

    pub fn uri(&self) -> &DocumentUri {
        &self.uri
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    pub fn version(&self) -> DocumentVersion {
        self.version
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn origin(&self) -> &DocumentOrigin {
        &self.origin
    }

    pub(crate) fn replace_text(&mut self, text: String, version: DocumentVersion) {
        self.text = text;
        self.version = version;
        self.origin = DocumentOrigin::Overlay;
    }
}
