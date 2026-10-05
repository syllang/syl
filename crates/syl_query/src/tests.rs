use crate::{AnalysisQueries, DiagnosticStage, QueryError, snapshot::DiagnosticQueryEngine};
use syl_hir::DefId;
use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryEndpoint,
    SummaryLatencyClass, SummaryLayout, SummaryPath, TrustBoundary,
};
use syl_session::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion};
use syl_span::SourcePosition;

#[test]
fn grouped_diagnostics_track_package_document_and_stage_boundaries() {
    let parse_uri = DocumentUri::new("untitled:syl/parse");
    let tir_uri = DocumentUri::new("untitled:syl/sema");
    let elab_uri = DocumentUri::new("untitled:syl/elab");
    let mut host = AnalysisHost::new();
    host.open_document(
        parse_uri.clone(),
        "cell Broken(".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        tir_uri.clone(),
        "cell Bad(x: in Missing) {}\n".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        elab_uri.clone(),
        "extern cell VendorLatch(y: in Bit)\n\ncell Top(y: out Bit) {\n    signal tmp: Bit := 0\n    let vendor = place VendorLatch(y: tmp)\n    y := tmp\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    host.register_opaque_summary(trusted_vendor_summary());

    let snapshot = host
        .snapshot()
        .expect("query grouped diagnostic fixture must snapshot");
    let grouped = snapshot.grouped_diagnostics();

    assert_eq!(grouped.packages().len(), 3);
    assert_document_has_stage(&grouped, &parse_uri, DiagnosticStage::Parse);
    assert_document_has_stage(&grouped, &tir_uri, DiagnosticStage::Tir);
    assert_document_lacks_stage(&grouped, &elab_uri, DiagnosticStage::Elaboration);
    assert!(
        snapshot.semantic_diagnostics().iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
        }),
        "CLI semantic diagnostics still report the vendor latch elaboration diagnostic"
    );
}

#[test]
fn hover_and_completion_do_not_trigger_elaboration() {
    let source = "cell Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/app");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("hover fixture must snapshot cleanly");

    let token = CancellationToken::new();
    let hover_position = source_position(source, "x\n}");
    let completion_position = source_position(source, "y := ");
    let hover = snapshot
        .hover_at_with_token(&uri, hover_position, &token)
        .expect("hover query should succeed");
    let completions = snapshot
        .completions_at_with_token(&uri, completion_position, &token)
        .expect("completion query should succeed");

    assert!(hover.is_some());
    assert!(!completions.items.is_empty());
}

#[test]
fn analysis_queries_expose_docs() {
    let source = r#"
//! Module docs.

/// Bundle docs.
bundle Word {
    /// Low bits.
    lo: UInt<4>,
}
"#;
    let uri = DocumentUri::new("untitled:syl/docs");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host.snapshot().expect("doc query fixture must snapshot");
    let file = snapshot
        .file_by_uri(&uri)
        .expect("snapshot should retain opened document");
    let word = DefId::new(0);

    assert_eq!(
        snapshot.doc_for_module(file.source_id()),
        Some("Module docs.")
    );
    assert_eq!(snapshot.doc_for_item(word), Some("Bundle docs."));
    assert_eq!(snapshot.doc_for_field(word, "lo"), Some("Low bits."));
}

#[test]
fn hover_includes_item_and_field_docs() {
    let source = r#"
/// Bundle docs.
bundle Word {
    /// Low bits.
    lo: UInt<4>,
}
"#;
    let uri = DocumentUri::new("untitled:syl/doc-hover");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host.snapshot().expect("doc hover fixture must snapshot");

    let item_hover = snapshot
        .hover_at(&uri, source_position(source, "Word"))
        .expect("bundle name should produce hover");
    assert_eq!(item_hover.contents, "bundle Word\n\nBundle docs.");

    let field_hover = snapshot
        .hover_at(&uri, source_position(source, "lo:"))
        .expect("field name should produce hover");
    assert_eq!(field_hover.contents, "field lo\n\nLow bits.");
}

#[test]
fn navigation_queries_use_target_package_semantic_shard() {
    let alpha_source = "cell Alpha(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let beta_source = "cell Beta(y: out Bit) {\n    y := 0\n}\n";
    let beta_updated_source = "cell Beta(y: out Bit) {\n    y := 1\n}\n";
    let alpha_uri = DocumentUri::new("untitled:syl/alpha");
    let beta_uri = DocumentUri::new("untitled:syl/beta");
    let mut host = AnalysisHost::new();
    host.open_document(
        alpha_uri.clone(),
        alpha_source.to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        beta_uri.clone(),
        beta_source.to_string(),
        DocumentVersion::new(1),
    );
    let initial_snapshot = host
        .snapshot()
        .expect("initial navigation fixture must snapshot cleanly");

    host.update_document_at_version(
        &beta_uri,
        beta_updated_source.to_string(),
        DocumentVersion::new(2),
    )
    .expect("beta update must succeed");
    let updated = host
        .snapshot()
        .expect("updated navigation fixture must snapshot cleanly");
    assert!(source_text(&initial_snapshot, &beta_uri).contains("y := 0"));
    assert!(source_text(&updated, &beta_uri).contains("y := 1"));
    assert!(source_text(&initial_snapshot, &alpha_uri).contains("y := x"));
    assert!(source_text(&updated, &alpha_uri).contains("y := x"));

    let token = CancellationToken::new();
    let hover = updated
        .hover_at_with_token(&alpha_uri, source_position(alpha_source, "x\n}"), &token)
        .expect("alpha hover should use package shard");
    let completions = updated
        .completions_at_with_token(&alpha_uri, source_position(alpha_source, "y := "), &token)
        .expect("alpha completion should use package shard");
    let definition = updated
        .definition_at_with_token(&alpha_uri, source_position(alpha_source, "x\n}"), &token)
        .expect("alpha definition should use package shard");

    assert!(hover.is_some());
    assert!(completions.items.iter().any(|item| item.label == "x"));
    assert!(definition.is_some());
}

#[test]
fn cancelled_grouped_diagnostics_do_not_start_semantic_stages() {
    let uri = DocumentUri::new("untitled:syl/app");
    let mut host = AnalysisHost::new();
    host.open_document(
        uri,
        "cell Top(y: out Bit) {\n    y := 1\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    let snapshot = host
        .snapshot()
        .expect("cancellation fixture must snapshot cleanly");
    let token = CancellationToken::new();
    token.cancel();

    let err = snapshot
        .grouped_diagnostics_with_token(&token)
        .expect_err("cancelled diagnostics query must stop before semantic work");

    assert_eq!(err, QueryError::Cancelled);
    snapshot
        .grouped_diagnostics_with_token(&CancellationToken::new())
        .expect("retry after cancel returns grouped diagnostics");
}

#[test]
fn cancelled_grouped_diagnostics_stop_before_later_package_semantics() {
    for attempt in 0..20 {
        let alpha_uri = DocumentUri::new(format!("untitled:syl/alpha{attempt}"));
        let beta_uri = DocumentUri::new(format!("untitled:syl/beta{attempt}"));
        let mut host = AnalysisHost::new();
        host.open_document(
            alpha_uri,
            "cell Alpha(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        host.open_document(
            beta_uri,
            "cell Beta(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        let snapshot = host
            .snapshot()
            .expect("multi-package cancellation fixture must snapshot cleanly");
        let token = CancellationToken::new();
        let mut seen = Vec::new();
        let err = DiagnosticQueryEngine::new(&snapshot)
            .grouped_diagnostics_observing_packages(&token, |package, token| {
                seen.push(package.to_string());
                if package == format!("alpha{attempt}") {
                    token.cancel();
                }
            })
            .expect_err("cancellation should stop grouped diagnostics before the next package");

        assert_eq!(err, QueryError::Cancelled);
        assert_eq!(seen, vec![format!("alpha{attempt}")]);
    }
}

#[test]
fn cancelled_hover_after_hir_cache_does_not_start_tir() {
    let source = "cell Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/app");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("hover cancellation fixture must snapshot cleanly");

    snapshot
        .hir_analysis_for_uri_with_token(&uri, &CancellationToken::new())
        .expect("package HIR should build before cancellation");
    let token = CancellationToken::new();
    token.cancel();

    let err = snapshot
        .hover_at_with_token(&uri, source_position(source, "x\n}"), &token)
        .expect_err("cancelled hover must not continue into TIR");

    assert_eq!(err, QueryError::Cancelled);
    snapshot
        .hir_analysis_for_uri_with_token(&uri, &token)
        .expect("a cached HIR hit still answers after the request is cancelled");
}

#[test]
fn completion_suppresses_invalid_assignment_cleanup_contexts() {
    let cases = [
        (
            "untitled:syl/next_eq",
            "cell Top(x: in Bit) {\n    next state = x\n}\n",
            "state = x",
        ),
        (
            "untitled:syl/signal_eq",
            "cell Top(x: in Bit) {\n    signal state: Bit = x\n}\n",
            "Bit = x",
        ),
        (
            "untitled:syl/let_drive",
            "cell Top(x: in Bit) {\n    let state := x\n}\n",
            "state := x",
        ),
    ];

    for (uri, source, marker) in cases {
        let uri = DocumentUri::new(uri);
        let mut host = AnalysisHost::new();
        host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
        let snapshot = host
            .snapshot()
            .expect("invalid completion fixture must snapshot cleanly");
        let completions = snapshot
            .completions_at_with_token(
                &uri,
                source_position(source, marker),
                &CancellationToken::new(),
            )
            .expect("invalid assignment cleanup context should still query successfully");

        assert!(
            completions.items.is_empty(),
            "old assignment syntax should not produce completions for {marker:?}: {:?}",
            completions.items
        );
    }
}

fn assert_document_lacks_stage(
    grouped: &crate::GroupedDiagnostics,
    uri: &DocumentUri,
    stage: DiagnosticStage,
) {
    let document = grouped
        .packages()
        .iter()
        .flat_map(|package| package.documents().iter())
        .find(|document| document.uri() == uri)
        .unwrap_or_else(|| panic!("missing diagnostic document for {}", uri));
    assert!(
        document.stages().iter().all(|item| item.stage() != stage),
        "IDE grouped diagnostics for {uri} must not include {stage:?}"
    );
}

fn assert_document_has_stage(
    grouped: &crate::GroupedDiagnostics,
    uri: &DocumentUri,
    stage: DiagnosticStage,
) {
    let document = grouped
        .packages()
        .iter()
        .flat_map(|package| package.documents().iter())
        .find(|document| document.uri() == uri)
        .unwrap_or_else(|| panic!("missing diagnostic document for {}", uri));
    let stages = document
        .stages()
        .iter()
        .map(|item| item.stage())
        .collect::<Vec<_>>();
    assert!(
        stages.contains(&stage),
        "expected {} diagnostics to include stage {:?}, got {:?}",
        uri,
        stage,
        stages
    );
}

fn source_text(snapshot: &syl_session::AnalysisSnapshot, uri: &DocumentUri) -> String {
    let file = snapshot
        .file_by_uri(uri)
        .unwrap_or_else(|| panic!("missing analysis file for {uri}"));
    snapshot
        .source_map()
        .file(file.source_id())
        .unwrap_or_else(|| panic!("missing source map entry for {uri}"))
        .text()
        .to_string()
}

fn source_position(source: &str, needle: &str) -> SourcePosition {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("query fixture must contain marker {needle:?}"));
    let prefix = &source[..offset];
    let line = prefix.lines().count().saturating_sub(1);
    let character = prefix
        .lines()
        .last()
        .map(|line| line.encode_utf16().count())
        .unwrap_or(0);
    SourcePosition::new(line, character)
}

fn trusted_vendor_summary() -> OpaqueItemSummary {
    OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorLatch")
        .endpoint(SummaryEndpoint::new(
            "y",
            SummaryDirection::In,
            SummaryLayout::Bit,
            SummaryCapability::Value,
        ))
        .driven_field(SummaryPath::new("y"))
        .latency_class(SummaryLatencyClass::Sequential)
        .trust_boundary(TrustBoundary::TrustedPrecompiled)
        .build()
}
