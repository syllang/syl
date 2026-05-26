use crate::{AnalysisQueries, DiagnosticStage, QueryError, snapshot::DiagnosticQueryEngine};
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
        "module Broken(".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        tir_uri.clone(),
        "module Bad(x: in Missing) {}\n".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        elab_uri.clone(),
        "extern module VendorLatch(y: in Bit)\n\nmodule Top(y: out Bit) {\n    signal tmp: Bit := 0\n    let vendor = place VendorLatch(y: tmp)\n    y := tmp\n}\n".to_string(),
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
    assert_document_has_stage(&grouped, &elab_uri, DiagnosticStage::Elaboration);
}

#[test]
fn hover_and_completion_do_not_trigger_elaboration() {
    let source = "module Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/app");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("hover fixture must snapshot cleanly");
    let app_cache = snapshot
        .package_semantic_cache("app")
        .expect("app package shard must exist");

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
    assert!(app_cache.is_hir_cached());
    assert!(app_cache.is_tir_cached());
    assert!(!snapshot.is_hir_cached());
    assert!(!snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
}

#[test]
fn navigation_queries_use_target_package_semantic_shard() {
    let alpha_source = "module Alpha(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let beta_source = "module Beta(y: out Bit) {\n    y := 0\n}\n";
    let beta_updated_source = "module Beta(y: out Bit) {\n    y := 1\n}\n";
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
    let baseline = host
        .snapshot()
        .expect("baseline navigation fixture must snapshot cleanly");
    let baseline_alpha = baseline
        .package_semantic_cache("alpha")
        .expect("alpha baseline shard must exist");
    let baseline_beta = baseline
        .package_semantic_cache("beta")
        .expect("beta baseline shard must exist");

    host.update_document_at_version(
        &beta_uri,
        beta_updated_source.to_string(),
        DocumentVersion::new(2),
    )
    .expect("beta update must succeed");
    let updated = host
        .snapshot()
        .expect("updated navigation fixture must snapshot cleanly");
    let updated_alpha = updated
        .package_semantic_cache("alpha")
        .expect("alpha updated shard must exist");
    let updated_beta = updated
        .package_semantic_cache("beta")
        .expect("beta updated shard must exist");

    assert!(baseline_alpha.shares_with(&updated_alpha));
    assert!(!baseline_beta.shares_with(&updated_beta));

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
    assert!(updated_alpha.is_hir_cached());
    assert!(updated_alpha.is_tir_cached());
    assert!(!updated_alpha.is_elaboration_cached());
    assert!(!updated_beta.is_hir_cached());
    assert!(!updated_beta.is_tir_cached());
    assert!(!updated_beta.is_elaboration_cached());
    assert!(!updated.is_hir_cached());
    assert!(!updated.is_tir_cached());
    assert!(!updated.is_elaboration_cached());
}

#[test]
fn cancelled_grouped_diagnostics_do_not_start_semantic_stages() {
    let uri = DocumentUri::new("untitled:syl/app");
    let mut host = AnalysisHost::new();
    host.open_document(
        uri,
        "module Top(y: out Bit) {\n    y := 1\n}\n".to_string(),
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
    assert!(!snapshot.is_hir_cached());
    assert!(!snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
}

#[test]
fn cancelled_grouped_diagnostics_stop_before_later_package_semantics() {
    for attempt in 0..20 {
        let alpha_uri = DocumentUri::new(format!("untitled:syl/alpha{attempt}"));
        let beta_uri = DocumentUri::new(format!("untitled:syl/beta{attempt}"));
        let mut host = AnalysisHost::new();
        host.open_document(
            alpha_uri,
            "module Alpha(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        host.open_document(
            beta_uri,
            "module Beta(y: out Bit) { y := 1 }\n".to_string(),
            DocumentVersion::new(1),
        );
        let snapshot = host
            .snapshot()
            .expect("multi-package cancellation fixture must snapshot cleanly");
        let alpha_cache = snapshot
            .package_semantic_cache(&format!("alpha{attempt}"))
            .expect("alpha package shard must exist");
        let beta_cache = snapshot
            .package_semantic_cache(&format!("beta{attempt}"))
            .expect("beta package shard must exist");
        let token = CancellationToken::new();
        let err = DiagnosticQueryEngine::new(&snapshot)
            .grouped_diagnostics_observing_packages(&token, |package, token| {
                if package == format!("alpha{attempt}") {
                    token.cancel();
                }
            })
            .expect_err("cancellation should stop grouped diagnostics before the next package");

        assert_eq!(err, QueryError::Cancelled);
        assert!(alpha_cache.is_hir_cached());
        assert!(!beta_cache.is_hir_cached());
        assert!(!beta_cache.is_tir_cached());
        assert!(!beta_cache.is_elaboration_cached());
    }
}

#[test]
fn cancelled_hover_after_hir_cache_does_not_start_tir() {
    let source = "module Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/app");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("hover cancellation fixture must snapshot cleanly");

    let app_cache = snapshot
        .package_semantic_cache("app")
        .expect("app package shard must exist");
    let _ = snapshot
        .hir_analysis_for_uri_with_token(&uri, &CancellationToken::new())
        .expect("package HIR should build before cancellation");
    assert!(app_cache.is_hir_cached());
    assert!(!app_cache.is_tir_cached());
    assert!(!snapshot.is_hir_cached());
    assert!(!snapshot.is_tir_cached());
    let token = CancellationToken::new();
    token.cancel();

    let err = snapshot
        .hover_at_with_token(&uri, source_position(source, "x\n}"), &token)
        .expect_err("cancelled hover must not continue into TIR");

    assert_eq!(err, QueryError::Cancelled);
    assert!(!app_cache.is_tir_cached());
    assert!(!snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
}

#[test]
fn completion_suppresses_invalid_assignment_cleanup_contexts() {
    let cases = [
        (
            "untitled:syl/next_eq",
            "module Top(x: in Bit) {\n    next state = x\n}\n",
            "state = x",
        ),
        (
            "untitled:syl/signal_eq",
            "module Top(x: in Bit) {\n    signal state: Bit = x\n}\n",
            "Bit = x",
        ),
        (
            "untitled:syl/let_drive",
            "module Top(x: in Bit) {\n    let state := x\n}\n",
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
