/// A segmented path used to identify definitions across packages.
///
/// Paths are dot-separated (e.g. `["std", "logic", "UInt"]`) and serve
/// as canonical names for name resolution and cross-referencing.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct HirPath {
    segments: Vec<String>,
}

impl HirPath {
    /// Creates a path from the given segments.
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    /// Returns the empty path (root).
    pub fn empty() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    /// Returns a new path with `name` appended as a leaf segment.
    pub fn with_leaf(&self, name: &str) -> Self {
        let mut segments = self.segments.clone();
        segments.push(name.to_string());
        Self { segments }
    }

    /// Returns the parent path (all segments except the last).
    pub fn parent(&self) -> Self {
        let mut segments = self.segments.clone();
        let _ = segments.pop();
        Self { segments }
    }

    /// Returns the last segment (the name), if any.
    pub fn leaf(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    /// Returns all segments of this path.
    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    /// Formats the path as a dot-separated string (e.g. `std.logic.UInt`).
    pub fn display(&self) -> String {
        if self.segments.is_empty() {
            return "<root>".to_string();
        }
        self.segments.join(".")
    }

    /// Returns the number of segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Returns `true` if this is the root path (empty).
    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    /// Returns `true` if this path is exactly one segment longer than `parent`
    /// and shares `parent` as a prefix.
    pub fn is_direct_child_of(&self, parent: &Self) -> bool {
        self.segments.len() == parent.segments.len() + 1
            && self.segments.starts_with(&parent.segments)
    }
}
