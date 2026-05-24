use crate::{AnalysisQueries, DiagnosticStage, QueryError};
use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryEndpoint,
    SummaryLatencyClass, SummaryLayout, SummaryPath, TrustBoundary,
};
use syl_session::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion};
use syl_span::SourcePosition;

#[test]
fn grouped_diagnostics_track_package_document_and_stage_boundaries() {
    let parse_uri = DocumentUri::new("untitled:syl/query-parse");
    let tir_uri = DocumentUri::new("untitled:syl/query-tir");
    let elab_uri = DocumentUri::new("untitled:syl/query-elab");
    let mut host = AnalysisHost::new();
    host.open_document(
        parse_uri.clone(),
        "package parse;\nmodule Broken(".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        tir_uri.clone(),
        "package sema;\nmodule Bad(x: in Missing) {}\n".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        elab_uri.clone(),
        "package elab;\nextern module VendorLatch(y: in Bit)\n\nmodule Top(y: out Bit) {\n    signal tmp: Bit := 0\n    inst vendor = VendorLatch(y: tmp)\n    y := tmp\n}\n".to_string(),
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
    let source = "package app;\nmodule Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/query-hover");
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
    assert!(snapshot.is_hir_cached());
    assert!(snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
}

#[test]
fn cancelled_grouped_diagnostics_do_not_start_semantic_stages() {
    let uri = DocumentUri::new("untitled:syl/query-cancel");
    let mut host = AnalysisHost::new();
    host.open_document(
        uri,
        "package app;\nmodule Top(y: out Bit) {\n    y := 1\n}\n".to_string(),
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
fn cancelled_hover_after_hir_cache_does_not_start_tir() {
    let source = "package app;\nmodule Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/query-cancel-hover");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("hover cancellation fixture must snapshot cleanly");

    let _ = snapshot.hir_analysis();
    assert!(snapshot.is_hir_cached());
    assert!(!snapshot.is_tir_cached());
    let token = CancellationToken::new();
    token.cancel();

    let err = snapshot
        .hover_at_with_token(&uri, source_position(source, "x\n}"), &token)
        .expect_err("cancelled hover must not continue into TIR");

    assert_eq!(err, QueryError::Cancelled);
    assert!(!snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());
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
