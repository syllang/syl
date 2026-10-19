//! Per-module elaboration reuse. Driver facts and DRC stay on the design query.

use std::fs;

use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryEndpoint,
    SummaryLayout, TrustBoundary,
};

use super::PairFixture;
use crate::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig};

#[test]
fn independent_cell_body_edit_reuses_the_other_module() {
    let fixture = PairFixture::new(
        "cell Beta(y: out Bit) { y := 1 }\n",
        "use lib.Beta\ncell Alpha(y: out Bit) { y := 1 }\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    elaborate(&older);
    let modules = older.execution_count("owner_module_eir(");
    let const_mir = older.execution_count("owner_const_mir(");
    let map_ir = older.execution_count("owner_map_ir(");
    let designs = older.execution_count("workspace_elaboration(");
    assert!(modules >= 2, "each cell builds its own module");
    assert!(const_mir >= 2);
    assert!(map_ir >= 2);
    assert!(designs >= 1);

    fixture.write_lib("cell Beta(y: out Bit) { y := 0 }\n");
    let newer = host.snapshot().expect("body edit snapshot");
    elaborate(&newer);
    assert_eq!(newer.execution_count("owner_module_eir("), modules + 1);
    assert_eq!(newer.execution_count("owner_const_mir("), const_mir + 1);
    assert_eq!(newer.execution_count("owner_map_ir("), map_ir + 1);
    assert_eq!(newer.execution_count("workspace_elaboration("), designs + 1);
    assert!(
        newer
            .elaboration_diagnostics_with_token(&CancellationToken::new())
            .expect("edited design")
            .is_empty()
    );
}

#[test]
fn place_child_body_edit_reuses_the_parent_module() {
    let fixture = PairFixture::new(
        "cell Child(x: in Bit, y: out Bit) { y := x }\n",
        "use lib.Child\ncell Top(x: in Bit, y: out Bit) {\n    signal tmp: Bit\n    let child = place Child(x: x, y: tmp)\n    y := tmp\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    elaborate(&older);
    assert_hw_modules(&older, &["Child", "Top"]);
    let modules = older.execution_count("owner_module_eir(");
    let designs = older.execution_count("workspace_elaboration(");
    assert!(modules >= 2);

    fixture.write_lib("cell Child(x: in Bit, y: out Bit) { y := 0 }\n");
    let newer = host.snapshot().expect("child body edit");
    elaborate(&newer);
    assert_eq!(newer.execution_count("owner_module_eir("), modules + 1);
    assert_eq!(newer.execution_count("workspace_elaboration("), designs + 1);
    assert_hw_modules(&newer, &["Child", "Top"]);
}

#[test]
fn inplace_child_body_edit_rebuilds_the_parent_module() {
    let fixture = PairFixture::new(
        "cell Child(y: out Bit) { y := 1 }\n",
        "use lib.Child\ncell Top(y: out Bit) {\n    let child = inplace Child(y: y)\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    elaborate(&older);
    assert_hw_modules(&older, &["Child", "Top"]);
    let modules = older.execution_count("owner_module_eir(");
    let designs = older.execution_count("workspace_elaboration(");
    assert!(modules >= 2);

    fixture.write_lib("cell Child(y: out Bit) { y := 0 }\n");
    let newer = host.snapshot().expect("inplace body edit");
    elaborate(&newer);
    assert_eq!(newer.execution_count("owner_module_eir("), modules + 2);
    assert_eq!(newer.execution_count("workspace_elaboration("), designs + 1);
    assert_hw_modules(&newer, &["Child", "Top"]);
}

#[test]
fn nested_const_fn_body_is_part_of_the_module() {
    let root = super::TempTree::new();
    let path = root.path.join("top.syl");
    fs::write(
        &path,
        "fn other() -> bool { return true }\nfn choose() -> bool { return other() }\ncell Top(y: out Bit) {\n    if choose() {\n        y := 1\n    } else {\n        y := 0\n    }\n}\n",
    )
    .unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![path]);
    let snapshot = host.snapshot().expect("const fn snapshot");
    let diagnostics = snapshot
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("const fn elaboration");
    assert!(
        diagnostics.is_empty(),
        "nested const fn must elaborate: {diagnostics:?}"
    );
    let rendered = snapshot
        .hwir()
        .expect("const fn hwir")
        .modules()
        .iter()
        .map(|module| format!("{:?}", module.items()))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        rendered.contains("Int(1)"),
        "true const branch must survive: {rendered}"
    );
    assert!(
        !rendered.contains("Int(0)"),
        "false const branch must be folded away: {rendered}"
    );
}

#[test]
fn type_error_skips_module_queries() {
    let mut host = AnalysisHost::new();
    host.open_document(
        DocumentUri::new("untitled:syl/type-error"),
        "cell Bad(y: out Bit) {\n    y := true\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    let snapshot = host.snapshot().expect("type error snapshot");
    let diagnostics = snapshot.semantic_diagnostics();
    assert!(
        !diagnostics.is_empty(),
        "bool assigned to Bit is a type error"
    );
    assert_eq!(snapshot.execution_count("workspace_elaboration("), 0);
    assert_eq!(snapshot.execution_count("owner_module_eir("), 0);
    assert_eq!(snapshot.execution_count("owner_const_mir("), 0);
    assert_eq!(snapshot.execution_count("owner_map_ir("), 0);
}

#[test]
fn opaque_overlay_reuses_module_stages() {
    let root = super::TempTree::new();
    let path = root.path.join("box.syl");
    fs::write(&path, "extern cell VendorBox(y: in Bit)\n").unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![path]);
    let older = host.snapshot().expect("before overlay");
    elaborate(&older);
    let modules = older.execution_count("owner_module_eir(");
    let const_mir = older.execution_count("owner_const_mir(");
    let map_ir = older.execution_count("owner_map_ir(");
    let designs = older.execution_count("workspace_elaboration(");
    assert!(modules >= 1);
    host.register_opaque_summary(
        OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorBox")
            .endpoint(SummaryEndpoint::new(
                "y",
                SummaryDirection::In,
                SummaryLayout::Bit,
                SummaryCapability::Value,
            ))
            .trust_boundary(TrustBoundary::VendorBlackBox {
                vendor: "acme".to_string(),
            })
            .build(),
    );
    let newer = host.snapshot().expect("after overlay");
    elaborate(&newer);
    assert_eq!(newer.execution_count("owner_module_eir("), modules);
    assert_eq!(newer.execution_count("owner_const_mir("), const_mir);
    assert_eq!(newer.execution_count("owner_map_ir("), map_ir);
    assert_eq!(newer.execution_count("workspace_elaboration("), designs + 1);
}

fn elaborate(snapshot: &crate::AnalysisSnapshot) {
    snapshot
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("elaboration");
}

fn assert_hw_modules(snapshot: &crate::AnalysisSnapshot, expected: &[&str]) {
    let diagnostics = snapshot
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("module diagnostics");
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let mut names = snapshot
        .hwir()
        .expect("hwir")
        .modules()
        .iter()
        .map(|module| module.name().to_string())
        .collect::<Vec<_>>();
    names.sort();
    let mut expected = expected
        .iter()
        .map(|name| (*name).to_string())
        .collect::<Vec<_>>();
    expected.sort();
    assert_eq!(names, expected);
}
