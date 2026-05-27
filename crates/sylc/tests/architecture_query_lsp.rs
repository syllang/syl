use std::{
    fs,
    path::{Path, PathBuf},
};

use syl_query::{AnalysisQueries, DiagnosticStage, QueryError};
use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryEndpoint,
    SummaryLatencyClass, SummaryLayout, SummaryPath, TrustBoundary,
};
use syl_session::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion};
use syl_span::SourcePosition;

#[test]
fn architecture_query_lsp_session_owns_workspace_and_package_scoped_cache_invalidation() {
    let first_uri = DocumentUri::new("untitled:syl/first");
    let second_uri = DocumentUri::new("untitled:syl/second");
    let mut host = AnalysisHost::new();
    host.open_document(
        first_uri.clone(),
        "cell First(y: out Bit) {\n    y := 1\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    host.open_document(
        second_uri.clone(),
        "cell Second(y: out Bit) {\n    y := 0\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    let initial_snapshot = host
        .snapshot()
        .expect("query/lsp combined package fixture must snapshot");
    let initial_first = initial_snapshot
        .package_semantic_cache("first")
        .expect("first package shard must exist");
    let initial_second = initial_snapshot
        .package_semantic_cache("second")
        .expect("second package shard must exist");

    host.update_document_at_version(
        &second_uri,
        "cell Second(y: out Bit) {\n    y := 1\n}\n".to_string(),
        DocumentVersion::new(2),
    )
    .expect("query/lsp second package update must succeed");
    let updated = host
        .snapshot()
        .expect("query/lsp updated package fixture must snapshot");
    let updated_first = updated
        .package_semantic_cache("first")
        .expect("first package shard must still exist");
    let updated_second = updated
        .package_semantic_cache("second")
        .expect("second package shard must still exist");
    let updated_packages = updated.workspace().package_graph().packages();
    assert_eq!(updated_packages.len(), 2);
    assert!(
        updated_packages.iter().any(|package| {
            package.name() == "first" && package.documents().contains(&first_uri)
        })
    );
    assert!(updated_packages.iter().any(|package| {
        package.name() == "second" && package.documents().contains(&second_uri)
    }));
    assert_eq!(
        updated.workspace().source_database().documents().len(),
        2,
        "workspace snapshot should stay session-owned and track live documents"
    );
    assert!(initial_first.shares_with(&updated_first));
    assert!(!initial_second.shares_with(&updated_second));
}

#[test]
fn architecture_query_lsp_navigation_uses_target_package_semantic_shard() {
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
        .expect("initial navigation fixture must snapshot");
    let initial_alpha = initial_snapshot
        .package_semantic_cache("alpha")
        .expect("alpha initial shard must exist");
    let initial_beta = initial_snapshot
        .package_semantic_cache("beta")
        .expect("beta initial shard must exist");

    host.update_document_at_version(
        &beta_uri,
        beta_updated_source.to_string(),
        DocumentVersion::new(2),
    )
    .expect("beta update must succeed");
    let updated = host
        .snapshot()
        .expect("updated navigation fixture must snapshot");
    let updated_alpha = updated
        .package_semantic_cache("alpha")
        .expect("alpha updated shard must exist");
    let updated_beta = updated
        .package_semantic_cache("beta")
        .expect("beta updated shard must exist");

    assert!(initial_alpha.shares_with(&updated_alpha));
    assert!(!initial_beta.shares_with(&updated_beta));

    let token = CancellationToken::new();
    let hover = updated
        .hover_at_with_token(&alpha_uri, source_position(alpha_source, "x\n}"), &token)
        .expect("hover should stay package-local");
    let completions = updated
        .completions_at_with_token(&alpha_uri, source_position(alpha_source, "y := "), &token)
        .expect("completion should stay package-local");
    let definition = updated
        .definition_at_with_token(&alpha_uri, source_position(alpha_source, "x\n}"), &token)
        .expect("definition should stay package-local");

    assert!(hover.is_some());
    assert!(completions.items.iter().any(|item| item.label == "x"));
    assert!(definition.is_some());
    assert!(updated_alpha.is_hir_cached());
    assert!(updated_alpha.is_tir_cached());
    assert!(!updated_beta.is_hir_cached());
    assert!(!updated_beta.is_tir_cached());
    assert!(!updated.is_hir_cached());
    assert!(!updated.is_tir_cached());
    assert!(!updated.is_elaboration_cached());
}

#[test]
fn architecture_query_lsp_query_surface_stays_on_compiler_facts() {
    let workspace = workspace_root();
    let query_root = workspace.join("crates/syl_query/src");
    let manifest = read_text(workspace.join("crates/syl_query/Cargo.toml"));
    assert!(!manifest.contains("tower-lsp"));
    assert!(!manifest.contains("syl_lsp"));
    assert!(!manifest.contains("url"));

    let mut violations = Vec::new();
    for path in rs_files_under(&query_root) {
        if path.ends_with("tests.rs") {
            continue;
        }
        let text = read_text(path.clone());
        for forbidden in [
            "AnalysisHost",
            "ProjectResolver",
            "WorkspaceSnapshot",
            "PackageGraph",
            "syl_lsp::",
            "tower_lsp::",
            "syl_elab::",
            "use syl_elab",
        ] {
            if text.contains(forbidden) {
                violations.push(format!(
                    "{} contains forbidden query boundary dependency {forbidden:?}",
                    relative_to_workspace(&workspace, &path)
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "query production sources must stay on compiler facts only.\n{}",
        violations.join("\n")
    );

    let api = read_text(query_root.join("snapshot/api.rs"));
    let navigation_impl = api
        .split("impl<'a> SnapshotQueryEngine<'a>")
        .nth(1)
        .and_then(|tail| tail.split("fn map_project_error").next())
        .expect("SnapshotQueryEngine implementation should exist");
    for forbidden in ["hir_analysis_with_token", "tir_analysis_with_token"] {
        assert!(
            !navigation_impl.contains(forbidden),
            "navigation queries must use URI package-local semantic accessors, not {forbidden}"
        );
    }
}

#[test]
fn architecture_query_lsp_grouped_diagnostics_and_partial_failure_stages_stay_distinct() {
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
    assert_document_has_stage(&grouped, &elab_uri, DiagnosticStage::Elaboration);
}

#[test]
fn architecture_query_lsp_hover_and_completion_do_not_emit_and_respect_cancellation() {
    let source = "cell Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/app");
    let mut query_host = AnalysisHost::new();
    query_host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = query_host
        .snapshot()
        .expect("hover fixture must snapshot cleanly");
    let app_cache = snapshot
        .package_semantic_cache("app")
        .expect("app package shard must exist");

    let token = CancellationToken::new();
    let hover = snapshot
        .hover_at_with_token(&uri, source_position(source, "x\n}"), &token)
        .expect("hover query should succeed");
    let completions = snapshot
        .completions_at_with_token(&uri, source_position(source, "y := "), &token)
        .expect("completion query should succeed");

    assert!(hover.is_some());
    assert!(!completions.items.is_empty());
    assert!(app_cache.is_hir_cached());
    assert!(app_cache.is_tir_cached());
    assert!(!snapshot.is_hir_cached());
    assert!(!snapshot.is_tir_cached());
    assert!(!snapshot.is_elaboration_cached());

    let cancel_uri = DocumentUri::new("untitled:syl/app");
    let mut cancel_host = AnalysisHost::new();
    cancel_host.open_document(
        cancel_uri.clone(),
        source.to_string(),
        DocumentVersion::new(1),
    );
    let cancelled_snapshot = cancel_host
        .snapshot()
        .expect("cancellation fixture must snapshot cleanly");
    let cancel_cache = cancelled_snapshot
        .package_semantic_cache("app")
        .expect("cancellation app shard must exist");
    let _ = cancelled_snapshot
        .hir_analysis_for_uri_with_token(&cancel_uri, &CancellationToken::new())
        .expect("package HIR should build before cancellation");
    let cancelled = CancellationToken::new();
    cancelled.cancel();
    let err = cancelled_snapshot
        .hover_at_with_token(&cancel_uri, source_position(source, "x\n}"), &cancelled)
        .expect_err("cancelled hover must stop before TIR/elaboration");

    assert_eq!(err, QueryError::Cancelled);
    assert!(cancel_cache.is_hir_cached());
    assert!(!cancel_cache.is_tir_cached());
    assert!(!cancelled_snapshot.is_hir_cached());
    assert!(!cancelled_snapshot.is_tir_cached());
    assert!(!cancelled_snapshot.is_elaboration_cached());
}

#[test]
fn architecture_query_lsp_lsp_adapter_stays_protocol_only() {
    let workspace = workspace_root();
    let adapter = read_text(workspace.join("crates/syl_lsp/src/adapter.rs"));
    let diagnostics = read_text(workspace.join("crates/syl_lsp/src/diagnostics.rs"));
    let lib = read_text(workspace.join("crates/syl_lsp/src/lib.rs"));

    assert!(
        adapter.contains("struct LspAdapter"),
        "query/LSP architecture expects an explicit LSP adapter boundary"
    );
    assert!(
        adapter.contains("GroupedDiagnostics") && diagnostics.contains("GroupedDiagnostics"),
        "LSP diagnostics should be driven from query grouped diagnostics"
    );
    for forbidden in ["syl_sema::", "syl_elab::", "syl_hw::"] {
        assert!(
            !adapter.contains(forbidden) && !lib.contains(forbidden),
            "LSP protocol layer must stay off compiler internals: found {forbidden}"
        );
    }
}

fn assert_document_has_stage(
    grouped: &syl_query::GroupedDiagnostics,
    uri: &DocumentUri,
    stage: DiagnosticStage,
) {
    let document = grouped
        .packages()
        .iter()
        .flat_map(|package| package.documents().iter())
        .find(|document| document.uri() == uri)
        .unwrap_or_else(|| panic!("missing grouped diagnostics for {}", uri));
    assert!(
        document.stages().iter().any(|item| item.stage() == stage),
        "expected {} diagnostics to include stage {:?}",
        uri,
        stage
    );
}

fn source_position(source: &str, needle: &str) -> SourcePosition {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("query/lsp fixture must contain marker {needle:?}"));
    let prefix = &source[..offset];
    let line = prefix.lines().count().saturating_sub(1);
    let character = prefix
        .lines()
        .last()
        .map(|line| line.encode_utf16().count())
        .unwrap_or(0);
    SourcePosition::new(line, character)
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|path| path.parent())
        .expect("sylc crate should be nested under workspace/crates")
        .to_path_buf()
}

fn read_text(path: PathBuf) -> String {
    fs::read_to_string(&path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn rs_files_under(root: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect_rs_files(root, &mut out);
    out
}

fn collect_rs_files(root: &Path, out: &mut Vec<PathBuf>) {
    let entries =
        fs::read_dir(root).unwrap_or_else(|err| panic!("failed to read {}: {err}", root.display()));
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|err| panic!("failed to read entry under {}: {err}", root.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_rs_files(&path, out);
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            out.push(path);
        }
    }
}

fn relative_to_workspace(workspace: &Path, path: &Path) -> String {
    path.strip_prefix(workspace)
        .unwrap_or(path)
        .display()
        .to_string()
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
