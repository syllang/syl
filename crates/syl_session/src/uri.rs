use std::{
    env, fmt,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
};
use url::Url;

/// Session document identity is the normalized URI string. Two values are equal
/// when their stored URI strings are byte-for-byte equal.
#[derive(Clone, Debug, PartialOrd, Ord)]
#[non_exhaustive]
pub struct DocumentUri {
    value: String,
}

impl DocumentUri {
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
