use std::{
    fs,
    path::{Path, PathBuf},
};

use syl_sema::{LoweringError, SemanticCompiler, TirError};
use syl_session::{AnalysisHost, DocumentUri, DocumentVersion};
use syl_span::Span;
use syl_syntax::SourceParser;

#[test]
fn architecture_semantic_readme_and_public_facts_facade_stay_explicit() {
    let workspace = workspace_root();
    let readme = read_text(&workspace.join("crates/syl_sema/README.md"));
    for required in [
        "ResolutionTable",
        "TypeTable",
        "CapabilityTable",
        "ConstFacts",
        "LayoutFacts",
        "ProtocolFacts",
        "facts facade",
    ] {
        assert!(
            readme.contains(required),
            "syl_sema README must document semantic fact ownership: missing {required:?}"
        );
    }

    let analysis = read_text(&workspace.join("crates/syl_sema/src/analysis.rs"));
    for required in [
        "pub fn resolution(&self) -> &ResolutionTable",
        "pub fn facts(&self) -> Option<&SemanticFacts>",
        "pub fn facts(&self) -> &SemanticFacts",
    ] {
        assert!(
            analysis.contains(required),
            "semantic analysis facade must expose semantic facts: missing {required:?}"
        );
    }
}

#[test]
fn architecture_semantic_hover_and_definition_queries_do_not_trigger_elaboration() {
    let source = r#"
cell Top(x: in Bit, y: out Bit) {
    y := x
}
"#;
    let x_offset = source.rfind('x').expect("query fixture must contain rhs x");
    let uri = DocumentUri::new("untitled:syl/semantic_query");
    let mut host = AnalysisHost::new();
    host.open_document(uri, source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("semantic query fixture must snapshot cleanly");
    assert!(!snapshot.is_hir_cached());
    assert!(!snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
    let file = snapshot
        .files()
        .first()
        .expect("snapshot must contain one analysis file");
    let span = Span::new_in(file.source_id(), x_offset, x_offset + 1);

    let definition = snapshot
        .hir_analysis()
        .definition_at(span)
        .expect("definition lookup must come from sema HIR facts");
    let hover = snapshot
        .tir_analysis()
        .and_then(|tir| tir.hover_at(span))
        .expect("hover lookup must come from sema TIR facts");

    assert_eq!(definition.name(), "x");
    assert!(hover.text().contains("Bit"));
    assert!(snapshot.is_hir_cached());
    assert!(snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
}

#[test]
fn architecture_semantic_query_layer_stays_on_snapshot_sema_accessors() {
    let query_api = read_text(&workspace_root().join("crates/syl_query/src/snapshot/api.rs"));

    for required in [
        "hir_analysis_for_uri_with_token(uri, token)",
        "tir_analysis_for_uri_with_token(uri, token)",
    ] {
        assert!(
            query_api.contains(required),
            "syl_query query path must read sema analyses directly: missing {required:?}"
        );
    }
    for forbidden in ["self.snapshot.hwir()", "use syl_elab", "output_for_tir"] {
        assert!(
            !query_api.contains(forbidden),
            "syl_query query path must not trigger elaboration: found {forbidden:?}"
        );
    }
}

#[test]
fn architecture_semantic_structured_errors_do_not_require_string_matching() {
    let file = SourceParser::new(
        r#"
cell Bad(x: in Missing) {
}
"#,
    )
    .parse_file()
    .expect("structured error fixture must parse");
    let files = [file];
    let session = SemanticCompiler::new().session(&files);
    let hir = session
        .resolve_hir()
        .expect("HIR must resolve before TIR error");
    let err = hir.check_tir().expect_err("unknown type must fail in sema");

    match err.kind() {
        LoweringError::Tir(TirError::UnknownType { name }) => assert_eq!(name, "Missing"),
        other => panic!("expected structured TirError::UnknownType, got {other:?}"),
    }
}

#[test]
fn architecture_semantic_fact_collectors_stay_canonical() {
    let workspace = workspace_root();
    let capability = normalize_whitespace(&read_text(
        &workspace.join("crates/syl_sema/src/facts/capability.rs"),
    ));
    for required in [
        "enum DomainFact",
        "BuiltinDomain",
        "Clock { domain: DomainFact }",
        "Reset { domain: DomainFact }",
    ] {
        assert!(
            capability.contains(required),
            "capability facts must keep canonical domain fact modeling explicit: missing {required:?}"
        );
    }
    for forbidden in ["known == target", "table.iter().find_map"] {
        assert!(
            !capability.contains(forbidden),
            "capability facts must not recover type identity by structural fallback: {forbidden:?}"
        );
    }

    let consts = normalize_whitespace(&read_text(
        &workspace.join("crates/syl_sema/src/facts/consts.rs"),
    ));
    for forbidden in [
        "ConstFactBuilder",
        "HirExprNode::",
        "BinaryOp::",
        "UnaryOp::",
        "const_binary_result",
    ] {
        assert!(
            !consts.contains(forbidden),
            "const facts must stay on the shared evaluator path, found old evaluator marker {forbidden:?}"
        );
    }
}

#[test]
fn architecture_semantic_production_code_stays_off_hardware_layers() {
    let workspace = workspace_root();
    let source_root = workspace.join("crates/syl_sema/src");
    let mut violations = Vec::new();

    for path in rs_files_under(&source_root) {
        let text = read_text(&path);
        for forbidden in [
            "use syl_elab",
            "syl_elab::",
            "use syl_hw",
            "syl_hw::",
            "use syl_emit",
            "syl_emit::",
        ] {
            if text.contains(forbidden) {
                violations.push(format!(
                    "{} imports forbidden hardware-layer dependency {forbidden:?}",
                    relative_to_workspace(&workspace, &path)
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "syl_sema production code must stay independent from elab/hw/emit.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_semantic_hardware_integration_tests_stay_inventoried() {
    let workspace = workspace_root();
    let mut actual: Vec<_> = rs_files_under(&workspace.join("crates/syl_sema/tests"))
        .into_iter()
        .filter(|path| {
            let text = read_text(path);
            [
                "use syl_elab",
                "syl_elab::",
                "use syl_hw",
                "syl_hw::",
                "use syl_emit",
                "syl_emit::",
            ]
            .iter()
            .any(|forbidden| text.contains(forbidden))
        })
        .map(|path| relative_to_workspace(&workspace, &path))
        .collect();
    actual.sort();

    assert_eq!(
        actual,
        vec![
            "crates/syl_sema/tests/alias_resolution.rs".to_string(),
            "crates/syl_sema/tests/bundle_resolution.rs".to_string(),
            "crates/syl_sema/tests/cell_summary.rs".to_string(),
            "crates/syl_sema/tests/const_resolution.rs".to_string(),
            "crates/syl_sema/tests/support/mod.rs".to_string(),
            "crates/syl_sema/tests/tir_semantics.rs".to_string(),
        ],
        "sema integration tests that still touch hardware layers must stay explicitly inventoried"
    );
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|path| path.parent())
        .expect("sylc crate should be nested under workspace/crates")
        .to_path_buf()
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn rs_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(root, &mut files);
    files.sort();
    files
}

fn collect_rs_files(root: &Path, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };

    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, files);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn relative_to_workspace(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .display()
        .to_string()
}
