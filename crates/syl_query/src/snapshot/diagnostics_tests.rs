use super::diagnostics::DiagnosticQueryEngine;
use crate::{AnalysisQueries, DiagnosticStage, QueryError};
use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryEndpoint,
    SummaryLatencyClass, SummaryLayout, SummaryPath, TrustBoundary,
};
use syl_session::{
    AnalysisDatabase, AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion,
};

fn fixture() -> (AnalysisSnapshot, DocumentUri) {
    let mut database = AnalysisDatabase::new();
    database.open_document(
        DocumentUri::new("untitled:syl/alpha"),
        "cell Broken(".into(),
        DocumentVersion::new(1),
    );
    let uri = DocumentUri::new("untitled:syl/beta");
    database.open_document(uri.clone(), "extern cell VendorLatch(y: in Bit)\ncell Top(y: out Bit) {\n signal tmp: Bit := 0\n let vendor = place VendorLatch(y: tmp)\n y := tmp\n}\n".into(), DocumentVersion::new(7));
    database.register_opaque_summary(
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
            .build(),
    );
    (database.snapshot().unwrap(), uri)
}

#[test]
fn background_is_explicit_elaboration_only_with_related_spans_and_versions() {
    let (snapshot, uri) = fixture();
    let fast = snapshot
        .grouped_diagnostics_with_token(&CancellationToken::new())
        .unwrap();
    assert!(
        fast.packages()
            .iter()
            .flat_map(|p| p.documents())
            .flat_map(|d| d.stages())
            .all(|s| s.stage() != DiagnosticStage::Elaboration)
    );
    let background = snapshot.background_elaboration_diagnostics();
    let documents = background
        .packages()
        .iter()
        .flat_map(|p| p.documents())
        .collect::<Vec<_>>();
    assert!(
        documents
            .iter()
            .flat_map(|d| d.stages())
            .all(|s| s.stage() == DiagnosticStage::Elaboration)
    );
    let doc = documents.iter().find(|d| d.uri() == &uri).unwrap();
    assert_eq!(doc.version(), Some(DocumentVersion::new(7)));
    let duplicate = doc
        .diagnostics()
        .iter()
        .find(|d| d.code() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .unwrap();
    assert!(!duplicate.related().is_empty());
    assert!(duplicate.related().iter().all(|r| r.uri() == &uri));
    let again = snapshot.grouped_diagnostics();
    assert!(
        again
            .packages()
            .iter()
            .flat_map(|p| p.documents())
            .flat_map(|d| d.stages())
            .all(|s| s.stage() != DiagnosticStage::Elaboration)
    );
}

#[test]
fn cancellation_before_work_and_between_packages_returns_no_partial_result() {
    let (snapshot, _) = fixture();
    let token = CancellationToken::new();
    token.cancel();
    assert_eq!(
        snapshot
            .background_elaboration_diagnostics_with_token(&token)
            .unwrap_err(),
        QueryError::Cancelled
    );
    let token = CancellationToken::new();
    let mut visited = Vec::new();
    let result = DiagnosticQueryEngine::new(&snapshot).background_elaboration_observing_packages(
        &token,
        |name, token| {
            visited.push(name.to_string());
            token.cancel();
        },
    );
    assert_eq!(result.unwrap_err(), QueryError::Cancelled);
    assert_eq!(visited, ["alpha"]);
    assert!(
        snapshot
            .background_elaboration_diagnostics_with_token(&CancellationToken::new())
            .is_ok()
    );
    let empty = AnalysisDatabase::new().snapshot().unwrap();
    assert_eq!(
        empty
            .background_elaboration_diagnostics_with_token(&token)
            .unwrap_err(),
        QueryError::Cancelled
    );
}
