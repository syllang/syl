use super::*;
use serde_json::{Value, json};
use syl_span::{DiagnosticRelatedInfo, SourceId};

fn render(sources: &SourceMap, diagnostics: &[Diagnostic], format: DiagnosticsFormat) -> String {
    DiagnosticRenderer::new(sources, Path::new("/project")).render(format, diagnostics)
}

#[test]
fn human_single_line_snippet_has_exact_underline_and_relative_path() {
    let mut sources = SourceMap::new();
    let id = sources.add_file("file:///project/src/top.syl", "cell Top {\n    bad\n}\n");
    let diagnostic = Diagnostic::new(Span::new_in(id, 15, 18), "invalid item").with_code("E_TEST");
    assert_eq!(
        render(&sources, &[diagnostic], DiagnosticsFormat::Human),
        "error[E_TEST]: invalid item\n  --> src/top.syl:2:5\n   |\n 2 |     bad\n   |     ^^^\n   |\nerror: aborting due to 1 previous errors\n"
    );
}

#[test]
fn human_multiline_tabs_utf8_and_crlf_align_and_clamp_safely() {
    let mut sources = SourceMap::new();
    let text = "\téx\r\nmid\r\n\tend\r\n";
    let id = sources.add_file("untitled:unicode", text);
    let diag = Diagnostic::new(Span::new_in(id, 2, 15), "range");
    let output = render(&sources, &[diag], DiagnosticsFormat::Human);
    assert!(output.contains("--> untitled:unicode:1:2"));
    assert!(output.contains(" 1 |     éx\n   |     ^^\n"));
    assert!(output.contains("   | ...\n"));
    assert!(output.contains(" 3 |     end\n   | ^^^^^^^\n"));
    assert!(!output.contains('\r'));
    for (start, end) in [(text.len(), text.len()), (999, 0), (4, 2), (2, 2)] {
        let diag = Diagnostic::new(Span::new_in(id, start, end), "clamped");
        let output = render(&sources, &[diag], DiagnosticsFormat::Human);
        assert!(output.contains('^'));
    }
    let empty = sources.add_file("untitled:empty", "");
    assert!(
        render(
            &sources,
            &[Diagnostic::new(Span::new_in(empty, 0, 0), "empty")],
            DiagnosticsFormat::Human
        )
        .contains(" 1 | \n   | ^\n")
    );
}

#[test]
fn human_related_notes_use_each_files_source_and_wavy_underlines() {
    let mut sources = SourceMap::new();
    let main = sources.add_file("file:///project/main.syl", "abc\ndef");
    let other = sources.add_file("file:///outside/other.syl", "\txyz");
    let diag = Diagnostic::new(Span::new_in(main, 0, 3), "duplicate")
        .with_related(DiagnosticRelatedInfo::new(
            Span::new_in(main, 4, 7),
            "same file",
        ))
        .with_related(DiagnosticRelatedInfo::new(
            Span::new_in(other, 1, 4),
            "other file",
        ));
    let output = render(&sources, &[diag], DiagnosticsFormat::Human);
    assert!(output.contains("note: same file\n  --> main.syl:2:1\n   |\n 2 | def\n   | ~~~"));
    assert!(output.contains(
        "note: other file\n  --> /outside/other.syl:1:2\n   |\n 1 |     xyz\n   |     ~~~"
    ));
}

#[test]
fn unknown_sources_keep_headers_and_notes_without_locations() {
    for id in [SourceId::UNKNOWN, SourceId::new(100)] {
        let diag = Diagnostic::new(Span::new_in(id, 0, 10), "unknown")
            .with_related(DiagnosticRelatedInfo::new(Span::default(), "related"));
        let output = render(&SourceMap::new(), &[diag], DiagnosticsFormat::Human);
        assert_eq!(
            output,
            "error: unknown\nnote: related\nerror: aborting due to 1 previous errors\n"
        );
    }
}

#[test]
fn json_schema_uses_scalar_columns_raw_bytes_and_related_locations() {
    let mut sources = SourceMap::new();
    let id = sources.add_file("file:///project/src/top.syl", "\té🙂x\nabc");
    let diag = Diagnostic::new(Span::new_in(id, 3, 7), "unicode")
        .with_code("E_TEST")
        .with_source("syl_elab")
        .with_related(DiagnosticRelatedInfo::new(
            Span::new_in(id, 9, 12),
            "previous",
        ))
        .with_related(DiagnosticRelatedInfo::new(Span::default(), "no source"));
    let output: Value =
        serde_json::from_str(&render(&sources, &[diag], DiagnosticsFormat::Json)).unwrap();
    assert_eq!(
        output,
        json!({"schema_version":1,"diagnostics":[{
            "severity":"error","code":"E_TEST","source":"syl_elab","message":"unicode",
            "location":{"file":"src/top.syl","uri":"file:///project/src/top.syl","byte_start":3,"byte_end":7,"start_line":1,"start_column":3,"end_line":1,"end_column":4},
            "related":[{"message":"previous","location":{"file":"src/top.syl","uri":"file:///project/src/top.syl","byte_start":9,"byte_end":12,"start_line":2,"start_column":1,"end_line":2,"end_column":4}}, {"message":"no source","location":null}]
        }]})
    );
    let empty: Value =
        serde_json::from_str(&render(&sources, &[], DiagnosticsFormat::Json)).unwrap();
    assert_eq!(empty, json!({"schema_version":1,"diagnostics":[]}));
}

#[test]
fn all_severities_and_project_errors_use_the_schema() {
    let sources = SourceMap::new();
    for (severity, human, json_label) in [
        (DiagnosticSeverity::Error, "error", "error"),
        (DiagnosticSeverity::Warning, "warning", "warning"),
        (DiagnosticSeverity::Information, "info", "information"),
        (DiagnosticSeverity::Hint, "hint", "hint"),
    ] {
        let mut diag = Diagnostic::new(Span::default(), "message").with_severity(severity);
        diag.source = None;
        let report: Value = serde_json::from_str(&render(
            &sources,
            std::slice::from_ref(&diag),
            DiagnosticsFormat::Json,
        ))
        .unwrap();
        assert_eq!(report["diagnostics"][0]["severity"], json_label);
        assert!(report["diagnostics"][0]["code"].is_null());
        assert!(report["diagnostics"][0]["source"].is_null());
        assert!(report["diagnostics"][0]["location"].is_null());
        assert_eq!(report["diagnostics"][0]["related"], json!([]));
        assert!(
            render(&sources, &[diag], DiagnosticsFormat::Human)
                .starts_with(&format!("{human}: message\n"))
        );
    }
    for (error, code) in [
        (
            ProjectError::ManifestParse {
                path: "syl.toml".into(),
                message: "syntax".into(),
            },
            "E_MANIFEST_PARSE",
        ),
        (
            ProjectError::ManifestInvalid {
                path: "syl.toml".into(),
                message: "blank".into(),
            },
            "E_MANIFEST_INVALID",
        ),
        (
            ProjectError::OpaqueSummaryParse {
                path: "s.json".into(),
                message: "syntax".into(),
            },
            "E_OPAQUE_SUMMARY_PARSE",
        ),
        (
            ProjectError::OpaqueSummaryInvalid {
                path: "s.json".into(),
                message: "version".into(),
            },
            "E_OPAQUE_SUMMARY_INVALID",
        ),
        (
            ProjectError::Read {
                path: "missing".into(),
                source: std::io::ErrorKind::NotFound.into(),
            },
            "E_PROJECT",
        ),
    ] {
        let diagnostic = project_error(error);
        let report: Value = serde_json::from_str(&render(
            &sources,
            std::slice::from_ref(&diagnostic),
            DiagnosticsFormat::Json,
        ))
        .unwrap();
        assert_eq!(report["diagnostics"][0]["code"], code);
        assert_eq!(report["diagnostics"][0]["source"], "syl_session");
        assert!(report["diagnostics"][0]["location"].is_null());
        assert!(
            render(&sources, &[*diagnostic], DiagnosticsFormat::Human)
                .starts_with(&format!("error[{code}]:"))
        );
    }
}
