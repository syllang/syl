mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use support::MiddleCompiler;
use syl_query::AnalysisQueries;
use syl_sema::{
    BackendConstraint, OpaqueItemKind, OpaqueItemSummary, OpaqueSummaryTable, SummaryCapability,
    SummaryDirection, SummaryEndpoint, SummaryLatencyClass, SummaryLayout, SummaryPath,
    TrustBoundary,
};
use syl_session::{AnalysisHost, DocumentUri, DocumentVersion};
use syl_syntax::SourceParser;

#[test]
fn architecture_opaque_public_summary_surface_stays_explicit() {
    let workspace = workspace_root();

    let sema_readme = read_text(&workspace.join("crates/syl_sema/README.md"));
    for required in [
        "OpaqueSummaryTable",
        "opaque_summaries()",
        "machine-readable opaque/public boundary summaries",
    ] {
        assert!(
            sema_readme.contains(required),
            "syl_sema README must document opaque summary ownership: missing {required:?}"
        );
    }

    let session_readme = read_text(&workspace.join("crates/syl_session/README.md"));
    for required in [
        "AnalysisSnapshot::opaque_summaries()",
        "Project::opaque_summaries()",
        "AnalysisHost::set_opaque_summaries()",
        "AnalysisHost::register_opaque_summary()",
    ] {
        assert!(
            session_readme.contains(required),
            "syl_session README must document snapshot summary access: missing {required:?}"
        );
    }

    let query_readme = read_text(&workspace.join("crates/syl_query/README.md"));
    assert!(
        query_readme.contains("AnalysisQueries::opaque_summaries()"),
        "syl_query README must document borrowed summary access"
    );

    let elab_readme = read_text(&workspace.join("crates/syl_elab/README.md"));
    assert!(
        elab_readme.contains("trusted opaque-summary inputs"),
        "syl_elab README must mention trusted opaque summary inputs"
    );

    let sema_analysis = read_text(&workspace.join("crates/syl_sema/src/analysis.rs"));
    let sema_facts = read_text(&workspace.join("crates/syl_sema/src/facts.rs"));
    let session_model = read_text(&workspace.join("crates/syl_session/src/snapshot/model.rs"));
    let session_host = read_text(&workspace.join("crates/syl_session/src/host.rs"));
    let query_api = read_text(&workspace.join("crates/syl_query/src/snapshot/api.rs"));
    let pipeline = read_text(&workspace.join("crates/syl_elab/src/pipeline.rs"));

    for required in [
        "pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable>",
        "pub fn opaque_summaries(&self) -> &OpaqueSummaryTable",
    ] {
        assert!(
            sema_analysis.contains(required) || sema_facts.contains(required),
            "sema public API must expose machine-readable opaque summaries: missing {required:?}"
        );
    }
    for required in [
        "pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable>",
        "pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable> {",
    ] {
        assert!(
            session_model.contains(required) || query_api.contains(required),
            "session/query public API must expose borrowed opaque summaries: missing {required:?}"
        );
    }
    for required in [
        "pub fn set_opaque_summaries(&mut self, opaque_summaries: OpaqueSummaryTable)",
        "pub fn register_opaque_summary(&mut self, summary: OpaqueItemSummary)",
    ] {
        assert!(
            session_host.contains(required),
            "session host must expose workspace-level opaque summary overlay registration: missing {required:?}"
        );
    }
    assert!(
        pipeline.contains("pub fn opaque_summaries(&self) -> Option<&OpaqueSummaryTable>"),
        "elaboration output must expose machine-readable opaque summaries"
    );
    for forbidden in ["use syl_elab", "output_for_tir(", "self.snapshot.hwir()"] {
        assert!(
            !query_api.contains(forbidden),
            "summary query surface must not turn syl_query into an elaboration DTO bucket: found {forbidden:?}"
        );
    }
}

#[test]
fn architecture_opaque_session_and_query_read_summary_without_elaboration() {
    let source = r#"
extern cell DriveBit(y: out Bit)
"#;
    let uri = DocumentUri::new("untitled:syl/opaque_summary");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host
        .snapshot()
        .expect("opaque summary fixture must snapshot cleanly");

    assert!(!snapshot.is_elaboration_cached());
    let from_session = snapshot
        .opaque_summaries()
        .expect("session snapshot must expose sema-owned opaque summaries");
    let from_query = AnalysisQueries::opaque_summaries(&snapshot)
        .expect("query trait must borrow the same opaque summary surface");
    let summary = from_session
        .get("DriveBit")
        .expect("extern summary must be present in sema facts");

    assert_eq!(
        summary,
        from_query.get("DriveBit").expect("query view must match")
    );
    assert!(matches!(summary.kind(), OpaqueItemKind::ExternCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::SourceDerived
    ));
    assert!(summary.consumed_fields().is_empty());
    assert_eq!(
        summary
            .driven_fields()
            .iter()
            .map(SummaryPath::display)
            .collect::<Vec<_>>(),
        vec!["y".to_string()]
    );
    let endpoint = summary
        .endpoints()
        .first()
        .expect("extern summary must keep endpoint metadata");
    assert_eq!(endpoint.name(), "y");
    assert!(matches!(endpoint.direction(), SummaryDirection::Out));
    assert!(matches!(endpoint.layout(), SummaryLayout::Bit));
    assert!(matches!(endpoint.capability(), SummaryCapability::Value));
    assert!(!snapshot.is_elaboration_cached());
}

#[test]
fn architecture_opaque_host_registered_overlay_is_visible_to_query_and_elab() {
    let source = r#"
extern cell VendorBox(y: in Bit)

cell Top(y: out Bit) {
    signal tmp: Bit := 0
    let vendor = place VendorBox(y: tmp)
    y := tmp
}
    "#;
    let uri = DocumentUri::new("untitled:syl/opaque_overlay");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let initial_snapshot = host
        .snapshot()
        .expect("initial overlay fixture must snapshot cleanly");
    let initial_summary = initial_snapshot
        .opaque_summaries()
        .and_then(|summaries| summaries.get("VendorBox"))
        .expect("extern summary must exist before overlay registration");
    assert!(matches!(initial_summary.kind(), OpaqueItemKind::ExternCell));
    assert!(matches!(
        initial_summary.trust_boundary(),
        TrustBoundary::SourceDerived
    ));

    host.register_opaque_summary(
        OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorBox")
            .endpoint(SummaryEndpoint::new(
                "y",
                SummaryDirection::In,
                SummaryLayout::Bit,
                SummaryCapability::Value,
            ))
            .driven_field(SummaryPath::new("y"))
            .latency_class(SummaryLatencyClass::Sequential)
            .trust_boundary(TrustBoundary::VendorBlackBox {
                vendor: "acme".to_string(),
            })
            .backend_constraint(BackendConstraint::RequiresBackend {
                backend: "systemverilog".to_string(),
            })
            .build(),
    );
    let snapshot = host
        .snapshot()
        .expect("overlay fixture must snapshot cleanly");

    assert!(
        !initial_snapshot.shares_semantic_cache_with(&snapshot),
        "registering a workspace overlay must invalidate the previous semantic cache"
    );
    assert!(!snapshot.is_elaboration_cached());
    let from_session = snapshot
        .opaque_summaries()
        .expect("snapshot must expose merged opaque summaries");
    let from_query = AnalysisQueries::opaque_summaries(&snapshot)
        .expect("query trait must borrow the same merged summary surface");
    let summary = from_session
        .get("VendorBox")
        .expect("host-registered overlay must merge into snapshot summaries");

    assert_eq!(
        summary,
        from_query
            .get("VendorBox")
            .expect("query surface must match")
    );
    assert!(matches!(summary.kind(), OpaqueItemKind::PrecompiledCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::VendorBlackBox { vendor } if vendor == "acme"
    ));
    assert_eq!(
        summary
            .driven_fields()
            .iter()
            .map(SummaryPath::display)
            .collect::<Vec<_>>(),
        vec!["y".to_string()]
    );
    assert!(matches!(
        summary.backend_constraints(),
        [BackendConstraint::RequiresBackend { backend }] if backend == "systemverilog"
    ));
    assert!(!snapshot.is_elaboration_cached());

    let duplicate = snapshot
        .semantic_diagnostics()
        .into_iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .expect("the same overlay visible to query must also feed elaboration DRC");

    assert!(
        duplicate
            .message
            .contains("duplicate hardware driver for tmp"),
        "overlay-driven DRC must stay structured across the session/elab boundary"
    );
    assert!(snapshot.is_elaboration_cached());
}

#[test]
fn architecture_opaque_extern_out_auto_drive_summary_enters_metadata() {
    let output = MiddleCompiler::new()
        .output_files(&[parse_file(
            r#"
extern cell DriveBit(y: out Bit)

cell Top(y: out Bit) {
    let drive = place DriveBit(y: y)
}
"#,
        )])
        .expect("extern output summary fixture must elaborate");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");
    let summary = metadata
        .opaque_summaries()
        .get("DriveBit")
        .expect("extern cell summary must be preserved in compilation metadata");

    assert!(matches!(summary.kind(), OpaqueItemKind::ExternCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::SourceDerived
    ));
    assert_eq!(
        summary
            .driven_fields()
            .iter()
            .map(SummaryPath::display)
            .collect::<Vec<_>>(),
        vec!["y".to_string()]
    );
    assert!(metadata.driver_facts().iter().any(|fact| {
        fact.module() == "Top"
            && (fact.target() == "y"
                || matches!(fact.target_place(), syl_hw::HwPlace::Object { name, .. } if name == "y"))
    }));
}

#[test]
fn architecture_opaque_precompiled_summary_without_body_enters_multi_driver_drc() {
    let source = r#"
extern cell VendorLatch(y: in Bit)

cell Top(y: out Bit) {
    signal tmp: Bit := 0
    let vendor = place VendorLatch(y: tmp)
    y := tmp
}
"#;
    let signature_only = MiddleCompiler::new()
        .output_files(&[parse_file(source)])
        .expect("signature-only opaque boundary should elaborate");
    assert!(
        signature_only
            .diagnostics()
            .iter()
            .all(|diagnostic| diagnostic.code.as_deref()
                != Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")),
        "input-only signature must not claim a driver without trusted summary"
    );

    let trusted = OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorLatch")
        .endpoint(SummaryEndpoint::new(
            "y",
            SummaryDirection::In,
            SummaryLayout::Bit,
            SummaryCapability::Value,
        ))
        .driven_field(SummaryPath::new("y"))
        .latency_class(SummaryLatencyClass::Sequential)
        .trust_boundary(TrustBoundary::TrustedPrecompiled)
        .build();
    let output = MiddleCompiler::with_opaque_summaries(OpaqueSummaryTable::from_iter([trusted]))
        .output_files(&[parse_file(source)])
        .expect("trusted precompiled summary should elaborate into DRC diagnostics");
    let duplicate = output
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .expect("trusted precompiled summary must feed a driver fact into multi-driver DRC");

    assert!(
        duplicate
            .message
            .contains("duplicate hardware driver for tmp"),
        "trusted summary should surface the same structured duplicate-driver diagnostic"
    );
}

#[test]
fn architecture_opaque_trust_boundaries_are_structured_and_assertable() {
    let summary = OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorBox")
        .endpoint(SummaryEndpoint::new(
            "y",
            SummaryDirection::Out,
            SummaryLayout::Bit,
            SummaryCapability::Value,
        ))
        .driven_field(SummaryPath::new("y"))
        .latency_class(SummaryLatencyClass::Sequential)
        .trust_boundary(TrustBoundary::VendorBlackBox {
            vendor: "acme".to_string(),
        })
        .backend_constraint(BackendConstraint::RequiresBackend {
            backend: "systemverilog".to_string(),
        })
        .backend_constraint(BackendConstraint::RequiresBlackBoxArtifact {
            artifact: "VendorBox.sv".to_string(),
        })
        .build();
    let output = MiddleCompiler::with_opaque_summaries(OpaqueSummaryTable::from_iter([summary]))
        .output_files(&[parse_file(
            r#"
extern cell VendorBox(y: out Bit)

cell Top(y: out Bit) {
    let vendor = place VendorBox(y: y)
}
"#,
        )])
        .expect("vendor summary fixture must elaborate");
    let metadata = output
        .metadata()
        .expect("clean vendor summary fixture must lower metadata");
    let summary = metadata
        .opaque_summaries()
        .get("VendorBox")
        .expect("merged opaque summary must be preserved in metadata");

    assert!(matches!(summary.kind(), OpaqueItemKind::PrecompiledCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::VendorBlackBox { vendor } if vendor == "acme"
    ));
    assert_eq!(summary.latency_class(), SummaryLatencyClass::Sequential);
    assert!(matches!(
        summary.backend_constraints(),
        [
            BackendConstraint::RequiresBackend { backend },
            BackendConstraint::RequiresBlackBoxArtifact { artifact },
        ] if backend == "systemverilog" && artifact == "VendorBox.sv"
    ));
}

fn parse_file(source: &str) -> syl_syntax::AstFile {
    SourceParser::new(source)
        .parse_file()
        .unwrap_or_else(|errors| panic!("test source must parse: {errors:?}"))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root must resolve")
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}
