use crate::cli::DiagnosticsFormat;
use serde::Serialize;
use std::{fmt::Write, path::Path};
use syl_session::{DocumentUri, ProjectError};
use syl_span::{Diagnostic, DiagnosticSeverity, SourceFile, SourceMap, Span};

pub(crate) struct DiagnosticRenderer<'a> {
    sources: &'a SourceMap,
    display_base: &'a Path,
}
impl<'a> DiagnosticRenderer<'a> {
    pub(crate) fn new(sources: &'a SourceMap, display_base: &'a Path) -> Self {
        Self {
            sources,
            display_base,
        }
    }

    pub(crate) fn render(&self, format: DiagnosticsFormat, diagnostics: &[Diagnostic]) -> String {
        match format {
            DiagnosticsFormat::Human => self.human(diagnostics),
            DiagnosticsFormat::Json => {
                let report = JsonReport {
                    schema_version: 1,
                    diagnostics: diagnostics
                        .iter()
                        .map(|d| JsonDiagnostic {
                            severity: severity(d.severity),
                            code: d.code.as_deref(),
                            source: d.source.as_deref(),
                            message: &d.message,
                            location: self.location(d.span),
                            related: d
                                .related
                                .iter()
                                .map(|rel| JsonRelated {
                                    message: &rel.message,
                                    location: self.location(rel.span),
                                })
                                .collect(),
                        })
                        .collect(),
                };
                // The report contains only strings, integers, arrays, and optional records.
                let mut output =
                    serde_json::to_string(&report).expect("diagnostic report is JSON serializable");
                output.push('\n');
                output
            }
        }
    }

    fn display_path(&self, file: &SourceFile) -> String {
        DocumentUri::new(file.uri()).to_file_path().map_or_else(
            || file.uri().to_string(),
            |path| {
                path.strip_prefix(self.display_base)
                    .unwrap_or(&path)
                    .display()
                    .to_string()
            },
        )
    }

    fn location(&self, span: Span) -> Option<JsonLocation> {
        let file = self.sources.file(span.source)?;
        let range = SourceRange::new(file.text(), span);
        Some(JsonLocation {
            file: self.display_path(file),
            uri: file.uri().into(),
            byte_start: span.start,
            byte_end: span.end,
            start_line: range.start_line + 1,
            start_column: range.start_column() + 1,
            end_line: range.end_line + 1,
            end_column: range.end_column() + 1,
        })
    }

    fn human(&self, diagnostics: &[Diagnostic]) -> String {
        let mut output = String::new();
        for diagnostic in diagnostics {
            let label = if diagnostic.severity == DiagnosticSeverity::Information {
                "info"
            } else {
                severity(diagnostic.severity)
            };
            let code = diagnostic
                .code
                .as_ref()
                .map(|code| format!("[{code}]"))
                .unwrap_or_default();
            writeln!(output, "{label}{code}: {}", diagnostic.message).unwrap();
            self.snippet(&mut output, diagnostic.span, '^');
            for related in &diagnostic.related {
                writeln!(output, "note: {}", related.message).unwrap();
                self.snippet(&mut output, related.span, '~');
            }
        }
        let errors = diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count();
        if errors > 0 {
            writeln!(output, "error: aborting due to {errors} previous errors").unwrap();
        }
        output
    }

    fn snippet(&self, output: &mut String, span: Span, marker: char) {
        let Some(file) = self.sources.file(span.source) else {
            return;
        };
        let range = SourceRange::new(file.text(), span);
        let width = (range.end_line + 1).to_string().len() + 1;
        let snippet = Snippet { width, marker };
        writeln!(
            output,
            "{:width$}--> {}:{}:{}",
            "",
            self.display_path(file),
            range.start_line + 1,
            range.start_column() + 1
        )
        .unwrap();
        writeln!(output, "{:width$} |", "").unwrap();
        let first = range.line(range.start_line);
        let start = range
            .start
            .saturating_sub(range.starts[range.start_line])
            .min(first.len());
        if range.start_line == range.end_line {
            let end = range
                .end
                .saturating_sub(range.starts[range.end_line])
                .min(first.len());
            snippet.line(output, range.start_line, first, start..end);
        } else {
            snippet.line(output, range.start_line, first, start..first.len());
            if range.end_line > range.start_line + 1 {
                writeln!(output, "{:width$} | ...", "").unwrap();
            }
            let last = range.line(range.end_line);
            let end = range
                .end
                .saturating_sub(range.starts[range.end_line])
                .min(last.len());
            snippet.line(output, range.end_line, last, 0..end);
        }
        writeln!(output, "{:width$} |", "").unwrap();
    }
}

struct Snippet {
    width: usize,
    marker: char,
}
impl Snippet {
    fn line(
        &self,
        output: &mut String,
        number: usize,
        source: &str,
        marked: std::ops::Range<usize>,
    ) {
        let width = self.width;
        writeln!(
            output,
            "{:>width$} | {}",
            number + 1,
            source.replace('\t', "    ")
        )
        .unwrap();
        let padding = " ".repeat(visual_width(&source[..marked.start]));
        let underline = self
            .marker
            .to_string()
            .repeat(visual_width(&source[marked]).max(1));
        writeln!(output, "{:width$} | {padding}{underline}", "").unwrap();
    }
}
fn visual_width(text: &str) -> usize {
    text.chars().map(|ch| if ch == '\t' { 4 } else { 1 }).sum()
}
fn severity(value: DiagnosticSeverity) -> &'static str {
    match value {
        DiagnosticSeverity::Error => "error",
        DiagnosticSeverity::Warning => "warning",
        DiagnosticSeverity::Information => "information",
        DiagnosticSeverity::Hint => "hint",
        _ => "error",
    }
}

/// A clamped UTF-8 range with scalar columns, independent of LSP UTF-16 mapping.
struct SourceRange<'a> {
    text: &'a str,
    starts: Vec<usize>,
    start: usize,
    end: usize,
    start_line: usize,
    end_line: usize,
}
impl<'a> SourceRange<'a> {
    fn new(text: &'a str, span: Span) -> Self {
        let clamp = |mut offset: usize| {
            offset = offset.min(text.len());
            while !text.is_char_boundary(offset) {
                offset -= 1;
            }
            offset
        };
        let start = clamp(span.start);
        let end = clamp(span.end.max(start));
        let starts: Vec<_> = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        let start_line = starts
            .partition_point(|&offset| offset <= start)
            .saturating_sub(1);
        let end_line = starts
            .partition_point(|&offset| offset <= end)
            .saturating_sub(1);
        Self {
            text,
            starts,
            start,
            end,
            start_line,
            end_line,
        }
    }
    fn start_column(&self) -> usize {
        self.text[self.starts[self.start_line]..self.start]
            .chars()
            .count()
    }
    fn end_column(&self) -> usize {
        self.text[self.starts[self.end_line]..self.end]
            .chars()
            .count()
    }
    fn line(&self, index: usize) -> &str {
        let end = self
            .starts
            .get(index + 1)
            .copied()
            .unwrap_or(self.text.len());
        let line = self.text[self.starts[index]..end].trim_end_matches('\n');
        line.strip_suffix('\r').unwrap_or(line)
    }
}

pub(crate) fn project_error(error: ProjectError) -> Box<Diagnostic> {
    let code = match &error {
        ProjectError::ManifestParse { .. } => "E_MANIFEST_PARSE",
        ProjectError::ManifestInvalid { .. } => "E_MANIFEST_INVALID",
        ProjectError::OpaqueSummaryParse { .. } => "E_OPAQUE_SUMMARY_PARSE",
        ProjectError::OpaqueSummaryInvalid { .. } => "E_OPAQUE_SUMMARY_INVALID",
        _ => "E_PROJECT",
    };
    Box::new(
        Diagnostic::new(Span::default(), error.to_string())
            .with_code(code)
            .with_source("syl_session"),
    )
}

#[derive(Serialize)]
struct JsonReport<'a> {
    schema_version: u32,
    diagnostics: Vec<JsonDiagnostic<'a>>,
}
#[derive(Serialize)]
struct JsonDiagnostic<'a> {
    severity: &'static str,
    code: Option<&'a str>,
    source: Option<&'a str>,
    message: &'a str,
    location: Option<JsonLocation>,
    related: Vec<JsonRelated<'a>>,
}
#[derive(Serialize)]
struct JsonRelated<'a> {
    message: &'a str,
    location: Option<JsonLocation>,
}
#[derive(Serialize)]
struct JsonLocation {
    file: String,
    uri: String,
    byte_start: usize,
    byte_end: usize,
    start_line: usize,
    start_column: usize,
    end_line: usize,
    end_column: usize,
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
