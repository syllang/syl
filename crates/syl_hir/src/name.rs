#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[non_exhaustive]
pub struct HirPath {
    segments: Vec<String>,
}

impl HirPath {
    pub fn new(segments: Vec<String>) -> Self {
        Self { segments }
    }

    pub fn empty() -> Self {
        Self {
            segments: Vec::new(),
        }
    }

    pub fn with_leaf(&self, name: &str) -> Self {
        let mut segments = self.segments.clone();
        segments.push(name.to_string());
        Self { segments }
    }

    pub fn parent(&self) -> Self {
        let mut segments = self.segments.clone();
        let _ = segments.pop();
        Self { segments }
    }

    pub fn leaf(&self) -> Option<&str> {
        self.segments.last().map(String::as_str)
    }

    pub fn segments(&self) -> &[String] {
        &self.segments
    }

    pub fn display(&self) -> String {
        if self.segments.is_empty() {
            return "<root>".to_string();
        }
        self.segments.join(".")
    }

    pub fn len(&self) -> usize {
        self.segments.len()
    }

    pub fn is_empty(&self) -> bool {
        self.segments.is_empty()
    }

    pub fn is_direct_child_of(&self, parent: &Self) -> bool {
        self.segments.len() == parent.segments.len() + 1
            && self.segments.starts_with(&parent.segments)
    }
}
