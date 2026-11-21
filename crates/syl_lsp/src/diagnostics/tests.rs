use super::*;
use std::{
    env, fs,
    time::{SystemTime, UNIX_EPOCH},
};
use syl_query::AnalysisQueries;
use syl_session::{AnalysisHost, ProjectResolver};
use syl_session::{DocumentUri, DocumentVersion};

#[test]
fn publications_carry_snapshot_file_versions() {
    let uri = DocumentUri::new("file:///tmp/syl_lsp_version.syl");
    let mut host = AnalysisHost::new();
    host.open_document(
        uri.clone(),
        "module Broken(".to_string(),
        DocumentVersion::new(7),
    );
    let snapshot = host
        .snapshot()
        .expect("overlay-only snapshot should be available");

    let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
        .publications()
        .into_iter()
        .find(|publication| publication.uri().as_str() == uri.as_str())
        .expect("snapshot file must produce a diagnostic publication");

    assert_eq!(publication.version(), Some(7));
}

#[test]
fn publications_preserve_facade_utf16_ranges() {
    let uri = DocumentUri::new("file:///tmp/syl_lsp_utf16.syl");
    let source = format!("const WIDTH: Nat = {}\n", '\u{1F4A1}');
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source, DocumentVersion::new(5));
    let snapshot = host
        .snapshot()
        .expect("UTF-16 diagnostic fixture should snapshot through parser recovery");
    let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
        .publications()
        .into_iter()
        .find(|publication| publication.uri().as_str() == uri.as_str())
        .expect("opened overlay must produce a diagnostic publication");
    let (_, diagnostics, version) = publication.into_parts();
    let lexer_diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.message.contains("unexpected character"))
        .expect("invalid non-ASCII token must publish a diagnostic");

    assert_eq!(version, Some(5));
    assert_eq!(lexer_diagnostic.range.start.line, 0);
    assert_eq!(
        lexer_diagnostic
            .range
            .end
            .character
            .saturating_sub(lexer_diagnostic.range.start.character),
        2
    );
}

#[test]
fn diagnostic_state_clears_files_missing_from_next_snapshot() {
    let first_uri = Url::parse("file:///tmp/syl_lsp_removed.syl").expect("test URL must parse");
    let second_uri = Url::parse("file:///tmp/syl_lsp_current.syl").expect("test URL must parse");
    let mut state = LspDiagnosticState::new();

    let first = state.reconcile(vec![LspDiagnosticPublication::new(
        first_uri.clone(),
        Some(1),
        Vec::new(),
    )]);
    let second = state.reconcile(vec![LspDiagnosticPublication::new(
        second_uri.clone(),
        Some(2),
        Vec::new(),
    )]);

    assert_eq!(first.len(), 1);
    assert!(
        second.iter().any(|publication| {
            publication.uri() == &first_uri && publication.version().is_none()
        })
    );
    assert!(second.iter().any(|publication| {
        publication.uri() == &second_uri && publication.version() == Some(2)
    }));
}

#[test]
fn disk_publications_do_not_carry_fake_versions() {
    let root = LspDiagnosticWorkspace::new();
    let path = root.write("broken.syl", "module Broken(");
    let snapshot = ProjectResolver::new()
        .load(&[path])
        .expect("diagnostic fixture must load through parser recovery")
        .snapshot()
        .clone();

    let publication = LspDiagnostics::new(&snapshot.grouped_diagnostics())
        .publications()
        .into_iter()
        .find(|publication| publication.uri().as_str().ends_with("broken.syl"))
        .expect("disk file must produce a publication");

    assert_eq!(publication.version(), None);
}

#[test]
fn publications_preserve_lsp_diagnostic_fields_and_related_locations() {
    let parse_uri = DocumentUri::new("file:///tmp/syl_lsp_related_parse.syl");
    let driver_uri = DocumentUri::new("file:///tmp/syl_lsp_related.syl");
    let mut host = AnalysisHost::new();
    host.open_document(
        parse_uri.clone(),
        "module Broken(".to_string(),
        DocumentVersion::new(11),
    );
    host.open_document(
        driver_uri.clone(),
        "cell Bad(y: out Bit) {\n    y := 0\n    y := 1\n}\n".to_string(),
        DocumentVersion::new(11),
    );
    let snapshot = host
        .snapshot()
        .expect("related diagnostic snapshot should build");
    let publications = LspDiagnostics::new(&snapshot.grouped_diagnostics()).publications();
    let publication = publications
        .iter()
        .find(|publication| publication.uri().as_str() == parse_uri.as_str())
        .expect("parse overlay must produce a diagnostic publication");
    let (published_uri, diagnostics, version) = publication.clone().into_parts();

    assert_eq!(published_uri.as_str(), parse_uri.as_str());
    assert_eq!(version, Some(11));
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic.code == Some(NumberOrString::String("E_SYNTAX_PARSE".to_string()))
        })
        .expect("parser recovery must publish a syntax diagnostic");
    assert_eq!(diagnostic.severity, Some(LspSeverity::ERROR));
    assert_eq!(diagnostic.source.as_deref(), Some("syl_syntax::parser"));
    assert!(
        publications.iter().all(|publication| {
            publication.clone().into_parts().1.iter().all(|diagnostic| {
                diagnostic.code
                    != Some(NumberOrString::String(
                        "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER".to_string(),
                    ))
            })
        }),
        "IDE publications must not include elaboration diagnostics"
    );
    let driver = snapshot
        .semantic_diagnostics()
        .into_iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .expect("CLI semantic diagnostics still report the duplicate driver");
    assert!(
        driver
            .related
            .iter()
            .any(|item| item.message == "previous driver claim")
    );
    assert!(
        driver
            .related
            .iter()
            .any(|item| item.message == "conflicting driver claim")
    );
}

#[test]
fn publications_cover_parse_tir_and_query_stage_failures() {
    let parse_uri = DocumentUri::new("file:///tmp/syl_lsp_parse.syl");
    let tir_uri = DocumentUri::new("file:///tmp/syl_lsp_tir.syl");
    let driver_uri = DocumentUri::new("file:///tmp/syl_lsp_driver.syl");
    let mut host = AnalysisHost::new();
    host.open_document(
        parse_uri.clone(),
        "module Broken(".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        tir_uri.clone(),
        "cell Bad(x: in Missing) {}\n".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        driver_uri.clone(),
        "cell Bad(y: out Bit) {\n    y := 0\n    y := 1\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    let snapshot = host
        .snapshot()
        .expect("partial diagnostic fixture must snapshot");
    let publications = LspDiagnostics::new(&snapshot.grouped_diagnostics()).publications();

    assert_publication_has_diagnostics(&publications, parse_uri.as_str());
    assert_publication_has_diagnostics(&publications, tir_uri.as_str());
    assert!(
        publications.iter().all(|publication| {
            publication.clone().into_parts().1.iter().all(|diagnostic| {
                diagnostic.code
                    != Some(NumberOrString::String(
                        "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER".to_string(),
                    ))
            })
        }),
        "IDE publications for {driver_uri} must not include elaboration"
    );
    assert!(
        snapshot.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
        }),
        "CLI semantic diagnostics still report the duplicate driver"
    );
}

#[non_exhaustive]
struct LspDiagnosticWorkspace {
    root: std::path::PathBuf,
}

impl LspDiagnosticWorkspace {
    fn new() -> Self {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after Unix epoch for test path")
            .as_nanos();
        let root = env::temp_dir().join(format!("syl_lsp_diag_{}_{}", std::process::id(), stamp));
        fs::create_dir_all(&root).expect("diagnostic workspace must be creatable");
        Self { root }
    }

    fn write(&self, relative: &str, text: &str) -> std::path::PathBuf {
        let path = self.root.join(relative);
        fs::write(&path, text).expect("diagnostic fixture must be writable");
        path
    }
}

fn assert_publication_has_diagnostics(publications: &[LspDiagnosticPublication], uri: &str) {
    let publication = publications
        .iter()
        .find(|publication| publication.uri().as_str() == uri)
        .unwrap_or_else(|| panic!("missing LSP publication for {uri}"));
    let (_, diagnostics, _) = publication.clone().into_parts();
    assert!(
        !diagnostics.is_empty(),
        "expected partial diagnostics for {uri}"
    );
}

fn layer(uri: &Url, version: Option<i32>, message: &str) -> LspDiagnosticPublication {
    LspDiagnosticPublication::new(
        uri.clone(),
        version,
        vec![LspDiagnostic {
            message: message.into(),
            ..Default::default()
        }],
    )
}

#[test]
fn layered_publication_merges_and_strictly_rejects_stale_generations_and_versions() {
    let uri = Url::parse("untitled:syl/layered").unwrap();
    let mut state = LspDiagnosticState::new();
    let fast = layer(&uri, Some(5), "parse");
    let elab = layer(&uri, Some(5), "driver");
    state.apply_fast_layer(10, vec![fast.clone()]);
    assert!(
        state
            .apply_elaboration_layer(9, vec![elab.clone()])
            .is_empty()
    );
    assert!(
        state
            .apply_elaboration_layer(11, vec![elab.clone()])
            .is_empty()
    );
    assert!(
        state
            .apply_elaboration_layer(10, vec![layer(&uri, Some(4), "old")])
            .is_empty()
    );
    assert!(
        state
            .apply_elaboration_layer(10, vec![layer(&uri, None, "disk")])
            .is_empty()
    );
    let publications = state.apply_elaboration_layer(10, vec![elab.clone()]);
    assert_eq!(publications.len(), 1);
    let (_, diagnostics, version) = publications[0].clone().into_parts();
    assert_eq!(version, Some(5));
    assert_eq!(
        diagnostics
            .iter()
            .map(|d| d.message.as_str())
            .collect::<Vec<_>>(),
        ["parse", "driver"]
    );
    assert!(state.apply_elaboration_layer(10, vec![elab]).is_empty());
    assert!(state.apply_fast_layer(9, vec![]).is_empty());
    let cleared = state.apply_elaboration_layer(10, vec![]);
    assert_eq!(cleared, vec![fast]);
    state.apply_elaboration_layer(10, vec![layer(&uri, Some(5), "driver")]);
    let fast_new = layer(&uri, Some(6), "new");
    assert_eq!(
        state.apply_fast_layer(11, vec![fast_new.clone()]),
        vec![fast_new]
    );
    assert!(
        state
            .apply_elaboration_layer(10, vec![layer(&uri, Some(5), "old")])
            .is_empty()
    );
}

#[test]
fn layered_removals_and_project_errors_clear_old_diagnostics() {
    let uri = Url::parse("untitled:syl/removed").unwrap();
    let manifest = Url::parse("file:///project/syl.toml").unwrap();
    let mut state = LspDiagnosticState::new();
    state.apply_fast_layer(
        1,
        vec![LspDiagnosticPublication::new(uri.clone(), Some(1), vec![])],
    );
    state.apply_elaboration_layer(1, vec![layer(&uri, Some(1), "driver")]);
    state.record_project_error(layer(&manifest, None, "manifest error"));
    let cleared = state.apply_fast_layer(2, vec![]);
    assert!(
        cleared
            .iter()
            .any(|p| p.uri() == &uri && p.diagnostics.is_empty())
    );
    assert!(
        cleared
            .iter()
            .any(|p| p.uri() == &manifest && p.diagnostics.is_empty())
    );
    state.apply_fast_layer(3, vec![layer(&uri, Some(3), "parse")]);
    state.apply_elaboration_layer(3, vec![layer(&uri, Some(3), "driver")]);
    state.record_project_error(layer(&uri, None, "project error"));
    assert!(!state.elab_by_uri.contains_key(uri.as_str()));
    assert!(
        state
            .apply_elaboration_layer(3, vec![layer(&uri, Some(3), "driver")])
            .is_empty()
    );
}
