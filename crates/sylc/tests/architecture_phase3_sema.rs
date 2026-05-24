use std::{
    fs,
    path::{Path, PathBuf},
};

use syl_sema::{LoweringError, SemanticCompiler, TirError};
use syl_session::{AnalysisHost, DocumentUri, DocumentVersion};
use syl_span::Span;
use syl_syntax::SourceParser;

#[test]
fn architecture_phase3_readme_and_public_facts_facade_stay_explicit() {
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
            "syl_sema README must document Phase 3 fact ownership: missing {required:?}"
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
            "semantic analysis facade must expose Phase 3 facts: missing {required:?}"
        );
    }
}

#[test]
fn architecture_phase3_hover_and_definition_queries_do_not_trigger_elaboration() {
    let source = r#"
module Top(x: in Bit, y: out Bit) {
    y := x
}
"#;
    let x_offset = source.rfind('x').expect("query fixture must contain rhs x");
    let uri = DocumentUri::new("untitled:syl/phase3-query");
    let mut host = AnalysisHost::new();
    host.open_document(uri, source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("phase3 query fixture must snapshot cleanly");
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
    let debug = format!("{snapshot:?}");

    assert_eq!(definition.name(), "x");
    assert!(hover.text().contains("Bit"));
    assert!(debug.contains("hir_cached: true"));
    assert!(debug.contains("tir_cached: true"));
    assert!(debug.contains("elaboration_cached: false"));
}

#[test]
fn architecture_phase3_query_layer_stays_on_snapshot_sema_accessors() {
    let query_api = read_text(&workspace_root().join("crates/syl_query/src/snapshot/api.rs"));

    for required in [
        "self.snapshot.hir_analysis()",
        "self.snapshot.tir_analysis()",
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
fn architecture_phase3_structured_errors_do_not_require_string_matching() {
    let file = SourceParser::new(
        r#"
module Bad(x: in Missing) {
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
