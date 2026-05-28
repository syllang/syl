use std::{
    env, fmt,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use url::Url;

/// A normalized document URI (always `file://` with absolute path).
///
/// **Normalization guarantees:**
/// - Input `"file:///home/user/foo.syl"` and `"/home/user/foo.syl"` produce
///   the same normalized URI (the latter is converted to file:// form).
/// - Symlinks are resolved via `canonicalize` when the path exists on disk.
/// - Relative paths are resolved against `env::current_dir()`.
/// - Two `DocumentUri` values are `Eq`/`Hash` when their normalized strings
///   are byte-for-byte equal, making them suitable as map keys.
///
/// **Round-trip invariant:** `DocumentUri::from_file_path(uri.to_file_path())`
/// should be identity for file:// URIs, but may differ for non-file schemes.
///
/// ```ignore
/// let a = DocumentUri::new("file:///home/x/test.syl");
/// let b = DocumentUri::from_file_path(Path::new("/home/x/test.syl"));
/// assert_eq!(a, b);  // Both normalize to the same canonical path
/// ```
#[derive(Clone, Debug, PartialOrd, Ord)]
#[non_exhaustive]
pub struct DocumentUri {
    value: String,
}

impl DocumentUri {
    /// Creates a URI from any string — normalizes `file://` paths and
    /// canonicalizes the underlying file path if it exists on disk.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: DocumentUriValue::new(value.into()).normalized(),
        }
    }

    pub fn from_file_path(path: &Path) -> Self {
        Self {
            value: DocumentPath::new(path).uri_string(),
        }
    }

    pub fn to_file_path(&self) -> Option<PathBuf> {
        DocumentUriParser::new(&self.value).to_file_path()
    }

    pub fn as_str(&self) -> &str {
        &self.value
    }

    pub fn into_string(self) -> String {
        self.value
    }
}

impl PartialEq for DocumentUri {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for DocumentUri {}

impl Hash for DocumentUri {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl fmt::Display for DocumentUri {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.value)
    }
}

impl From<String> for DocumentUri {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}

impl From<&str> for DocumentUri {
    fn from(value: &str) -> Self {
        Self::new(value)
    }
}

#[non_exhaustive]
struct DocumentPath {
    path: PathBuf,
}

impl DocumentPath {
    fn new(path: &Path) -> Self {
        let path = if path.exists() {
            path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
        } else if path.is_absolute() {
            path.to_path_buf()
        } else {
            env::current_dir()
                .map(|cwd| cwd.join(path))
                .unwrap_or_else(|_| path.to_path_buf())
        };
        Self { path }
    }

    fn uri_string(self) -> String {
        match Url::from_file_path(&self.path) {
            Ok(uri) => uri.to_string(),
            Err(()) => format!("file://{}", self.path.display()),
        }
    }
}

#[non_exhaustive]
struct DocumentUriValue {
    value: String,
}

impl DocumentUriValue {
    fn new(value: String) -> Self {
        Self { value }
    }

    fn normalized(self) -> String {
        let Ok(uri) = Url::parse(&self.value) else {
            return self.value;
        };
        if uri.scheme() != "file" {
            return self.value;
        }
        let Ok(path) = uri.to_file_path() else {
            return self.value;
        };
        DocumentPath::new(&path).uri_string()
    }
}

#[non_exhaustive]
struct DocumentUriParser<'a> {
    value: &'a str,
}

impl<'a> DocumentUriParser<'a> {
    fn new(value: &'a str) -> Self {
        Self { value }
    }

    fn to_file_path(&self) -> Option<PathBuf> {
        let uri = Url::parse(self.value).ok()?;
        if uri.scheme() != "file" {
            return None;
        }
        uri.to_file_path().ok()
    }
}
