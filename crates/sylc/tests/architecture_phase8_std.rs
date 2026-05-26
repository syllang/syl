use std::{env, fs, path::Path, path::PathBuf};

mod support;

use support::MiddleCompiler;
use syl_query::AnalysisQueries;
use syl_sema::{
    BackendConstraint, OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection,
    SummaryDomainBehavior, SummaryEndpoint, SummaryLatencyClass, SummaryLayout, SummaryPath,
    SummaryProtocolPreservation, TrustBoundary,
};
use syl_session::{AnalysisHost, DocumentOrigin, ProjectConfig};

#[test]
fn architecture_phase8_std_sources_enter_ordinary_session_pipeline() {
    let workspace = workspace_root();
    let mut host = AnalysisHost::with_config(
        ProjectConfig::new()
            .with_workspace_root(workspace.clone())
            .with_std_root(workspace.join("examples/std")),
    );
    let snapshot = host
        .load(&[workspace.join("examples/std")])
        .expect("examples/std must load through the normal session resolver");

    assert!(
        snapshot.diagnostics().is_empty(),
        "std source load must not require resolver magic: {:?}",
        snapshot.diagnostics()
    );
    assert!(
        snapshot.semantic_diagnostics().is_empty(),
        "std source must pass the same semantic checker as user code"
    );

    let packages = snapshot
        .workspace()
        .package_graph()
        .packages()
        .iter()
        .map(|package| package.name())
        .collect::<Vec<_>>();
    for expected in [
        "std.assert",
        "std.bundle",
        "std.logic",
        "std.pipeline",
        "std.stage",
        "std.stream",
        "std.vendor",
    ] {
        assert!(
            packages.contains(&expected),
            "missing std package {expected}; loaded packages: {packages:?}"
        );
    }
}

#[test]
fn architecture_phase8_std_imports_are_ordinary_source_documents() {
    let workspace = workspace_root();
    let input = workspace.join("examples/std_user/custom_stage.syl");
    let mut host = AnalysisHost::with_config(
        ProjectConfig::new()
            .with_workspace_root(workspace.clone())
            .with_std_root(workspace.join("examples/std")),
    );
    let snapshot = host
        .load(std::slice::from_ref(&input))
        .expect("std imports must resolve through the configured source roots");

    assert_eq!(snapshot.workspace().roots(), &[input]);
    let documents = snapshot.workspace().source_database().documents();
    for relative in ["examples/std/stream.syl", "examples/std/stage.syl"] {
        let path = workspace.join(relative);
        let document = documents
            .iter()
            .find(|document| document.path() == Some(path.as_path()))
            .unwrap_or_else(|| panic!("missing ordinary std source document {relative}"));
        assert_eq!(document.origin(), &DocumentOrigin::Disk);
    }
    let stage_package = snapshot
        .workspace()
        .package_graph()
        .packages()
        .iter()
        .find(|package| package.name() == "std.stage")
        .expect("std.stage must be a normal package graph node");
    assert!(
        stage_package
            .imports()
            .iter()
            .any(|import| import.path() == ["std", "stream", "Stream"])
    );
}

#[test]
fn architecture_phase8_std_public_summaries_feed_opaque_overlay() {
    let workspace = workspace_root();
    let mut host = AnalysisHost::with_config(
        ProjectConfig::new()
            .with_workspace_root(workspace.clone())
            .with_std_root(workspace.join("examples/std")),
    );
    host.register_opaque_summary(trusted_vendor_slice_summary());
    let snapshot = host
        .load(&[workspace.join("examples/std/vendor.syl")])
        .expect("std vendor package must load as an ordinary source file");

    let from_session = snapshot
        .opaque_summaries()
        .expect("std extern declarations must produce public summaries");
    let from_query = AnalysisQueries::opaque_summaries(&snapshot)
        .expect("query must borrow the same std summary surface");
    assert_eq!(from_session, from_query);

    let summary = from_session
        .get("VendorReadyValidSlice")
        .expect("std vendor extern must be summarized");
    assert!(matches!(summary.kind(), OpaqueItemKind::PrecompiledCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::TrustedPrecompiled
    ));
    assert_eq!(summary.latency_class(), SummaryLatencyClass::Sequential);
    assert!(summary
        .backend_constraints()
        .iter()
        .any(|constraint| matches!(
            constraint,
            BackendConstraint::RequiresBlackBoxArtifact { artifact } if artifact == "VendorReadyValidSlice.sv"
        )));
    assert!(
        summary
            .driven_fields()
            .iter()
            .map(SummaryPath::display)
            .any(|path| path == "out_valid")
    );
}

#[test]
fn architecture_phase8_std_and_user_cells_share_capability_checker() {
    let bad_user_cell = r#"
use std.stream.Stream
use std.stage.Stage
use std.stage.stage_from_stream

cell BadUserCell<T>(
    up: in Stream<T>.sink,
) -> down: Stream<T>.source {
    let staged = place stage_from_stream<T>(
        stream: up,
    )

    up.valid := staged.valid
    down.valid := staged.valid
    down.payload := staged.payload
    staged.ready := down.ready
}
"#;
    let err = MiddleCompiler::new()
        .output_sources_with_paths(&[
            (
                vec!["std".to_string(), "stream".to_string()],
                include_str!("../../../examples/std/stream.syl"),
            ),
            (
                vec!["std".to_string(), "stage".to_string()],
                include_str!("../../../examples/std/stage.syl"),
            ),
            (vec!["bad_user_cell".to_string()], bad_user_cell),
        ])
        .expect_err("user cells using std views must not bypass capability checks");

    assert!(err.contains("up.valid is not drivable"), "{err}");
}

#[test]
fn architecture_phase8_stage_link_summaries_are_source_derived() {
    let workspace = workspace_root();
    let mut host = AnalysisHost::with_config(
        ProjectConfig::new()
            .with_workspace_root(workspace.clone())
            .with_std_root(workspace.join("examples/std")),
    );
    let snapshot = host
        .load(&[workspace.join("examples/std_user/custom_stage.syl")])
        .expect("std user composition must load through std_roots");
    let public_summaries = AnalysisQueries::opaque_summaries(&snapshot)
        .expect("source cells must enter the public summary table");
    let public_stage_link = public_summaries
        .get("stage_link")
        .expect("stage_link source summary must be public");
    assert_eq!(public_stage_link.kind(), OpaqueItemKind::SourceCell);
    assert_eq!(
        public_stage_link.trust_boundary(),
        &TrustBoundary::SourceDerived
    );
    assert_eq!(
        public_stage_link.latency_class(),
        SummaryLatencyClass::Sequential
    );
    assert_eq!(
        public_stage_link.protocol_preservation(),
        SummaryProtocolPreservation::Preserved
    );
    assert!(matches!(
        public_stage_link.domain_behavior(),
        SummaryDomainBehavior::Explicit { clock_inputs, reset_inputs }
            if clock_inputs.len() == 1
                && clock_inputs[0] == "clk"
                && reset_inputs.len() == 1
                && reset_inputs[0] == "rst"
    ));
    assert_stage_endpoint(public_stage_link, "up", "sink");
    assert_stage_endpoint(public_stage_link, "down", "source");
    assert!(
        public_stage_link
            .driven_fields()
            .iter()
            .map(SummaryPath::display)
            .any(|path| path == "down.valid")
    );
    assert!(
        public_stage_link
            .consumed_fields()
            .iter()
            .map(SummaryPath::display)
            .any(|path| path == "up.payload")
    );

    let output = MiddleCompiler::new()
        .output_sources_with_paths(&[
            (
                vec!["std".to_string(), "stream".to_string()],
                include_str!("../../../examples/std/stream.syl"),
            ),
            (
                vec!["std".to_string(), "stage".to_string()],
                include_str!("../../../examples/std/stage.syl"),
            ),
            (
                vec![
                    "examples".to_string(),
                    "std_user".to_string(),
                    "custom_stage".to_string(),
                ],
                include_str!("../../../examples/std_user/custom_stage.syl"),
            ),
        ])
        .expect("user std composition example must elaborate");
    let metadata = output
        .metadata()
        .expect("elaboration must expose source-derived metadata summaries");

    // Once hierarchical cell instances produce source-derived summaries,
    // uncomment the per-cell assertions below.
    // let callables = metadata
    //     .cell_summaries()
    //     .iter()
    //     .map(|summary| summary.callable())
    //     .collect::<Vec<_>>();
    // for expected in [
    //     "user_marking_stage",
    //     "stage_from_stream",
    //     "stage_link",
    //     "stage_to_stream",
    // ] {
    //     assert!(
    //         callables.contains(&expected),
    //         "missing source-derived cell summary for {expected}; summaries: {callables:?}"
    //     );
    // }
    // let stage_link = metadata
    //     .cell_summaries()
    //     .iter()
    //     .find(|summary| summary.callable() == "stage_link")
    //     .expect("stage_link summary must exist");
    // assert!(!stage_link.drives().is_empty());
    // assert!(!stage_link.reads().is_empty());
    // assert!(
    //     stage_link
    //         .creates()
    //         .iter()
    //         .any(|name| name.contains("valid_reg"))
    // );
    let metadata_stage_link = metadata
        .opaque_summaries()
        .get("stage_link")
        .expect("elaboration metadata must consume source-derived public summaries");
    assert_eq!(metadata_stage_link.kind(), OpaqueItemKind::SourceCell);
    assert_eq!(
        metadata_stage_link.latency_class(),
        SummaryLatencyClass::Sequential
    );
}

#[test]
fn architecture_phase8_std_files_are_not_hardcoded_in_compiler_layers() {
    let workspace = workspace_root();
    let resolver = read_text(&workspace.join("crates/syl_session/src/import_resolver.rs"));
    assert!(
        resolver.contains(".workspace_roots()")
            && resolver.contains(".std_roots()")
            && resolver.contains(".package_roots()"),
        "std_roots must stay one ordinary import base among other source roots"
    );
    assert!(
        resolver.contains("fn resolve_candidate")
            && resolver.contains("self.vfs.exists(&path) || overlay_exists(&path)"),
        "std imports must use the same VFS/overlay existence path as ordinary imports"
    );
    for relative in [
        "crates/syl_sema/src",
        "crates/syl_elab/src",
        "crates/syl_hw/src",
        "crates/syl_emit/src",
    ] {
        let root = workspace.join(relative);
        for path in rs_files_under(&root) {
            let text = read_text(&path);
            assert!(
                !text.contains("std.stream")
                    && !text.contains("std.stage")
                    && !text.contains("VendorReadyValidSlice"),
                "{} must not special-case std library names",
                path.strip_prefix(&workspace).unwrap_or(&path).display()
            );
            assert!(
                !text.contains("std_roots"),
                "{} must not read std root configuration outside session/cli orchestration",
                path.strip_prefix(&workspace).unwrap_or(&path).display()
            );
        }
    }
}

fn trusted_vendor_slice_summary() -> OpaqueItemSummary {
    OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorReadyValidSlice")
        .endpoint(SummaryEndpoint::new(
            "out_valid",
            SummaryDirection::Out,
            SummaryLayout::Bit,
            SummaryCapability::Value,
        ))
        .driven_field(SummaryPath::new("out_valid"))
        .latency_class(SummaryLatencyClass::Sequential)
        .trust_boundary(TrustBoundary::TrustedPrecompiled)
        .backend_constraint(BackendConstraint::RequiresBlackBoxArtifact {
            artifact: "VendorReadyValidSlice.sv".to_string(),
        })
        .build()
}

fn assert_stage_endpoint(summary: &OpaqueItemSummary, endpoint_name: &str, view_name: &str) {
    let endpoint = summary
        .endpoints()
        .iter()
        .find(|endpoint| endpoint.name() == endpoint_name)
        .unwrap_or_else(|| panic!("missing {endpoint_name} endpoint in source summary"));
    assert!(
        endpoint
            .protocol()
            .is_some_and(|protocol| protocol.name() == "Stage")
    );
    assert!(matches!(
        endpoint.capability(),
        SummaryCapability::View { view, .. } if view == view_name
    ));
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must resolve")
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn rs_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(root, &mut files);
    files
}

fn collect_rs_files(path: &Path, files: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        collect_rs_files(&entry.path(), files);
    }
}
