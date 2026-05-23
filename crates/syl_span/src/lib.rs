use std::fmt;

/// Source-map file identity.
///
/// Equality, ordering, and hashing are based only on the numeric slot allocated
/// by one `SourceMap`. The value is not stable across independent compiler
/// sessions unless an embedding layer explicitly remaps it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub struct SourceId(pub usize);

impl SourceId {
    pub const UNKNOWN: Self = Self(usize::MAX);

    pub fn new(value: usize) -> Self {
        Self(value)
    }

    pub fn get(self) -> usize {
        self.0
    }

    pub fn is_unknown(self) -> bool {
        self == Self::UNKNOWN
    }
}

impl Default for SourceId {
    fn default() -> Self {
        Self::UNKNOWN
    }
}

/// Byte span in a registered source file.
///
/// Equality, ordering, and hashing compare `(source, start, end)` exactly. This
/// is a source-map coordinate, not a semantic node identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
#[non_exhaustive]
pub struct Span {
    pub source: SourceId,
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self::new_in(SourceId::default(), start, end)
    }

    pub fn new_in(source: SourceId, start: usize, end: usize) -> Self {
        Self { source, start, end }
    }

    pub fn join(self, other: Span) -> Self {
        Self {
            source: self.source,
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceFile {
    id: SourceId,
    uri: String,
    text: String,
    line_starts: Vec<usize>,
}

impl SourceFile {
    pub fn new(id: SourceId, uri: impl Into<String>, text: impl Into<String>) -> Self {
        let text = text.into();
        let line_starts = Self::line_starts_for(&text);
        Self {
            id,
            uri: uri.into(),
            text,
            line_starts,
        }
    }

    pub fn id(&self) -> SourceId {
        self.id
    }

    pub fn uri(&self) -> &str {
        &self.uri
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn utf16_range(&self, span: Span) -> Option<SourceRange> {
        if span.source != self.id {
            return None;
        }
        Some(SourceRange::new(
            self.utf16_position(span.start),
            self.utf16_position(span.end),
        ))
    }

    pub fn byte_offset_for_utf16_position(&self, position: SourcePosition) -> usize {
        let line_start = match self.line_starts.get(position.line) {
            Some(start) => *start,
            None => return self.text.len(),
        };
        let line_end = self.line_end(position.line);
        let mut character = 0usize;
        for (relative, ch) in self.text[line_start..line_end].char_indices() {
            if character >= position.character {
                return line_start + relative;
            }
            character = character.saturating_add(ch.len_utf16());
        }
        line_end
    }

    fn utf16_position(&self, offset: usize) -> SourcePosition {
        let offset = self.clamp_to_char_boundary(offset.min(self.text.len()));
        let line = self
            .line_starts
            .partition_point(|line_start| *line_start <= offset)
            .saturating_sub(1);
        let line_start = self.line_starts.get(line).copied().unwrap_or_default();
        let character = self.text[line_start..offset]
            .chars()
            .map(char::len_utf16)
            .sum();
        SourcePosition::new(line, character)
    }

    fn clamp_to_char_boundary(&self, mut offset: usize) -> usize {
        while offset > 0 && !self.text.is_char_boundary(offset) {
            offset = offset.saturating_sub(1);
        }
        offset
    }

    fn line_end(&self, line: usize) -> usize {
        let next_line_start = self.line_starts.get(line.saturating_add(1)).copied();
        next_line_start
            .map(|start| start.saturating_sub(1))
            .unwrap_or_else(|| self.text.len())
    }

    fn line_starts_for(text: &str) -> Vec<usize> {
        let mut line_starts = vec![0];
        for (idx, ch) in text.char_indices() {
            if ch == '\n' {
                line_starts.push(idx + ch.len_utf8());
            }
        }
        line_starts
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceMap {
    files: Vec<SourceFile>,
}

impl SourceMap {
    pub fn new() -> Self {
        Self { files: Vec::new() }
    }

    pub fn add_file(&mut self, uri: impl Into<String>, text: impl Into<String>) -> SourceId {
        let id = SourceId::new(self.files.len());
        self.files.push(SourceFile::new(id, uri, text));
        id
    }

    pub fn file(&self, id: SourceId) -> Option<&SourceFile> {
        if id.is_unknown() {
            return None;
        }
        self.files.get(id.get())
    }

    pub fn files(&self) -> &[SourceFile] {
        &self.files
    }

    pub fn utf16_range(&self, span: Span) -> Option<SourceRange> {
        self.file(span.source)?.utf16_range(span)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourcePosition {
    pub line: usize,
    pub character: usize,
}

impl SourcePosition {
    pub fn new(line: usize, character: usize) -> Self {
        Self { line, character }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct SourceRange {
    pub start: SourcePosition,
    pub end: SourcePosition,
}

impl SourceRange {
    pub fn new(start: SourcePosition, end: SourcePosition) -> Self {
        Self { start, end }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Diagnostic {
    pub span: Span,
    pub severity: DiagnosticSeverity,
    pub code: Option<String>,
    pub source: Option<String>,
    pub message: String,
    pub related: Vec<DiagnosticRelatedInfo>,
}

impl Diagnostic {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            severity: DiagnosticSeverity::Error,
            code: None,
            source: Some("syl".to_string()),
            message: message.into(),
            related: Vec::new(),
        }
    }

    pub fn with_severity(mut self, severity: DiagnosticSeverity) -> Self {
        self.severity = severity;
        self
    }

    pub fn with_code(mut self, code: impl Into<String>) -> Self {
        self.code = Some(code.into());
        self
    }

    pub fn with_source(mut self, source: impl Into<String>) -> Self {
        self.source = Some(source.into());
        self
    }

    pub fn with_related(mut self, related: DiagnosticRelatedInfo) -> Self {
        self.related.push(related);
        self
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}..{}: {}",
            self.span.start, self.span.end, self.message
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Information,
    Hint,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct DiagnosticRelatedInfo {
    pub span: Span,
    pub message: String,
}

impl DiagnosticRelatedInfo {
    pub fn new(span: Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }
}
