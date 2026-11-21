use crate::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion, ProjectError};

#[test]
fn packages_isolate_parse_hir_and_tir_errors_from_hardware_diagnostics() {
    let mut host = AnalysisHost::new();
    let bad = DocumentUri::new("untitled:syl/bad");
    let good = DocumentUri::new("untitled:syl/good");
    host.open_document(bad.clone(), "cell Broken(".into(), DocumentVersion::new(1));
    host.open_document(
        good.clone(),
        "cell Top(y: out Bit) {\n y := 0\n y := 1\n}\n".into(),
        DocumentVersion::new(2),
    );
    let token = CancellationToken::new();
    let snapshot = host.snapshot().unwrap();
    assert_eq!(
        snapshot
            .package_elaboration_diagnostics_with_token(&bad, &token)
            .unwrap(),
        Some(vec![])
    );
    let diagnostics = snapshot
        .package_elaboration_diagnostics_with_token(&good, &token)
        .unwrap()
        .unwrap();
    assert!(
        diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")),
        "{diagnostics:?}"
    );
    assert_eq!(
        snapshot
            .package_elaboration_diagnostics_with_token(
                &DocumentUri::new("untitled:syl/missing"),
                &token
            )
            .unwrap(),
        None
    );
    assert_eq!(snapshot.execution_count("workspace_elaboration("), 0);
    token.cancel();
    assert!(matches!(
        snapshot.package_elaboration_diagnostics_with_token(&good, &token),
        Err(ProjectError::Cancelled)
    ));
    for text in [
        "cell Bad(x: in Missing) {}",
        "cell Bad(y: out Bit) { y := true }",
    ] {
        host.update_document(&bad, text.into()).unwrap();
        let snapshot = host.snapshot().unwrap();
        assert_eq!(
            snapshot
                .package_elaboration_diagnostics_with_token(&bad, &CancellationToken::new())
                .unwrap(),
            Some(vec![])
        );
    }
}
