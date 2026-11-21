use crate::{DocumentUri, ProjectError};
use syl_span::{SourcePosition, SourceRange};

#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
/// A full replacement or incremental edit using UTF-16 line/character positions.
pub struct TextDocumentEdit {
    range: Option<SourceRange>,
    text: String,
}

impl TextDocumentEdit {
    pub fn full(text: impl Into<String>) -> Self {
        Self {
            range: None,
            text: text.into(),
        }
    }
    pub fn incremental(range: SourceRange, text: impl Into<String>) -> Self {
        Self {
            range: Some(range),
            text: text.into(),
        }
    }
    pub fn range(&self) -> Option<SourceRange> {
        self.range
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

fn offset(text: &str, position: SourcePosition) -> usize {
    let mut start = 0;
    for (line, segment) in text.split_inclusive('\n').enumerate() {
        if line == position.line {
            let content = segment
                .strip_suffix("\r\n")
                .or_else(|| segment.strip_suffix('\n'))
                .unwrap_or(segment);
            let mut column = 0;
            for (index, ch) in content.char_indices() {
                column += ch.len_utf16();
                if column > position.character {
                    return start + index;
                }
            }
            return start + content.len();
        }
        start += segment.len();
    }
    text.len()
}

pub(crate) fn apply(
    uri: &DocumentUri,
    current: &str,
    edits: &[TextDocumentEdit],
) -> Result<String, ProjectError> {
    let mut buffer = current.to_owned();
    for edit in edits {
        if let Some(range) = edit.range() {
            let start = offset(&buffer, range.start);
            let end = offset(&buffer, range.end);
            if start > end {
                return Err(ProjectError::InvalidTextEdit {
                    uri: uri.to_string(),
                    message: "edit start position exceeds end position".into(),
                });
            }
            buffer.replace_range(start..end, edit.text());
        } else {
            buffer.clone_from(&edit.text);
        }
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests;
