//! Shipped host and snapshot paths for the Salsa cutover.
//!
//! Counters come from the test-only execution log. They are not a public cache probe.

mod cancel_tests;
mod concurrency_tests;
mod cycle_tests;
mod elab_tests;
mod owner_tests;
mod perf_tests;

use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::thread;

use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, SummaryCapability, SummaryDirection, SummaryEndpoint,
    SummaryLayout, TrustBoundary,
};

use crate::{
    AnalysisHost, AnalysisSnapshot, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig,
    ProjectError,
};

#[test]
fn body_only_edit_reuses_declaration_consumers() {
    let fixture = PairFixture::new(
        "fn helper() -> nat { return 1 }\n",
        "use lib.helper\ncell Top(y: out Bit) {\n    let n = helper()\n    y := 1\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let declarations = older.execution_count("declaration_contract(");
    let signatures = older.execution_count("symbol_signature(");
    assert!(owners >= 1, "hover must execute the owner query");
    assert!(
        declarations >= 1,
        "the owner must read the imported declaration"
    );
    assert!(
        signatures >= 1,
        "owner lowering must read the imported declaration signature"
    );
    let lib_uri = uri_for_module(&older, fixture.lib_name);
    let source_id = older.file_by_uri(&lib_uri).expect("lib file").source_id();
    let old_text = older.input_text(lib_uri.as_str()).expect("old lib text");
    let parsed = older.execution_count("parsed_file(");
    let contracts = older.execution_count("file_contracts(");
    let owner_hirs = older.execution_count("owner_hir(");
    let ide_hir = older.execution_count("ide_unit_hir(");
    let ide_tir = older.execution_count("ide_unit_tir(");

    fixture.write_lib("fn helper() -> nat { return 2 }\n");
    let newer = host.snapshot().expect("body edit snapshot");
    let app_uri = uri_for_module(&newer, "app");
    read_owner_through_navigation(&newer, "app");
    let staged = newer
        .package_stage_diagnostics_with_token(&app_uri, &CancellationToken::new())
        .expect("IDE diagnostics")
        .expect("app package stages");
    assert!(staged.elaboration().is_empty());
    assert_eq!(newer.execution_count("workspace_elaboration("), 0);
    assert_eq!(newer.execution_count("unit_elaboration("), 0);
    assert_eq!(newer.execution_count("hardware_contract("), 0);
    assert_eq!(newer.execution_count("owner_tir("), owners);
    assert_eq!(newer.execution_count("owner_hir("), owner_hirs);
    assert_eq!(newer.execution_count("declaration_contract("), declarations);
    assert_eq!(newer.execution_count("ide_unit_hir("), ide_hir);
    assert_eq!(newer.execution_count("ide_unit_tir("), ide_tir);
    assert_eq!(newer.execution_count("parsed_file("), parsed + 1);
    assert_eq!(newer.execution_count("file_contracts("), contracts + 1);
    assert_eq!(older.execution_count("owner_tir("), owners);
    assert!(newer.execution_count("file_item_index(") >= 1);
    let _ = newer.hir_analysis();
    let cli_tir = newer.tir_analysis();
    assert!(cli_tir.is_some(), "CLI typecheck returns owner TIR");
    assert_eq!(newer.execution_count("workspace_elaboration("), 0);
    assert_eq!(newer.execution_count("unit_elaboration("), 0);
    assert_eq!(newer.execution_count("hardware_contract("), 0);
    assert!(newer.execution_count("declaration_contract(") >= declarations);
    assert!(newer.execution_count("parsed_file(") > parsed);
    assert!(newer.execution_count("workspace_hir(") >= 1);
    assert!(newer.execution_count("owner_hir(") > owner_hirs);
    assert!(newer.execution_count("owner_tir(") > owners);
    assert_eq!(
        older.input_text(lib_uri.as_str()).as_deref(),
        Some(old_text.as_str())
    );
    assert!(
        older
            .source_map()
            .file(source_id)
            .expect("old source")
            .text()
            .contains("return 1")
    );
    assert!(
        newer
            .input_text(lib_uri.as_str())
            .expect("new lib text")
            .contains("return 2")
    );
}

#[test]
fn signature_edit_recomputes_declaration_consumers() {
    let fixture = PairFixture::new(
        "fn helper() -> nat { return 1 }\n",
        "use lib.helper\ncell Top(y: out Bit) {\n    let n = helper()\n    y := 1\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let declarations = older.execution_count("declaration_contract(");
    assert!(owners >= 1);
    assert!(declarations >= 1);
    let lib_uri = uri_for_module(&older, fixture.lib_name);

    fixture.write_lib("fn helper(x: nat) -> nat { return 1 }\n");
    let newer = host.snapshot().expect("signature edit snapshot");
    read_owner_through_navigation(&newer, "app");
    assert!(newer.execution_count("owner_tir(") > owners);
    assert!(newer.execution_count("declaration_contract(") > declarations);
    assert!(
        older
            .input_text(lib_uri.as_str())
            .expect("old signature")
            .contains("fn helper()")
    );
    assert!(
        newer
            .input_text(lib_uri.as_str())
            .expect("new signature")
            .contains("fn helper(x: nat)")
    );
}

#[test]
fn storage_edit_recomputes_hardware_fact_only() {
    let fixture = PairFixture::new(
        "cell Buf(y: out Bit) { y := 1 }\n",
        "use buf.Buf\ncell Top(y: out Bit) { y := 1 }\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    read_owner_through_navigation(&older, "app");
    let _ = older.semantic_diagnostics();
    let owners = older.execution_count("owner_tir(");
    let hardware = older.execution_count("hardware_contract(");
    let declarations = older.execution_count("declaration_contract(");
    assert!(owners >= 1, "hover must execute the owner query");
    assert!(
        hardware >= 1,
        "package diagnostics must read the hardware fact"
    );
    assert!(declarations >= 1);
    let lib_uri = uri_for_module(&older, fixture.lib_name);

    fixture.write_lib("cell Buf(y: out Bit) { reg x: Bit; y := x }\n");
    let newer = host.snapshot().expect("storage edit snapshot");
    read_owner_through_navigation(&newer, "app");
    let _ = newer.semantic_diagnostics();
    assert!(newer.execution_count("owner_tir(") > owners);
    assert!(newer.execution_count("owner_tir(") < owners.saturating_add(4));
    assert_eq!(newer.execution_count("declaration_contract("), declarations);
    assert!(newer.execution_count("hardware_contract(") > hardware);
    assert!(
        older
            .input_text(lib_uri.as_str())
            .expect("old body")
            .contains("y := 1")
    );
    assert!(
        newer
            .input_text(lib_uri.as_str())
            .expect("stored body")
            .contains("reg x")
    );
}

#[test]
fn transitive_signature_edit_recomputes_the_importing_owner() {
    let root = TempTree::new();
    fs::write(root.path.join("c.syl"), "const W: nat = 4\n").unwrap();
    fs::write(
        root.path.join("b.syl"),
        "use c.W\nfn helper(x: UInt<W>) -> UInt<W> { return x }\n",
    )
    .unwrap();
    fs::write(
        root.path.join("a.syl"),
        "use b.helper\ncell Top(y: out Bit) {\n    let n = helper(0)\n    y := 1\n}\n",
    )
    .unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![root.path.join("a.syl")]);
    let older = host.snapshot().expect("transitive snapshot");
    assert!(older.files().len() >= 3, "transitive import loads c");
    read_owner_through_navigation(&older, "a");
    let owners = older.execution_count("owner_tir(");
    let signatures = older.execution_count("symbol_signature(");
    assert!(owners >= 1);
    assert!(
        signatures >= 3,
        "owner lowering must read the transitive declaration signatures"
    );
    let const_uri = uri_for_module(&older, "c");

    fs::write(root.path.join("c.syl"), "const W: nat = 8\n").unwrap();
    let newer = host.snapshot().expect("const edit snapshot");
    read_owner_through_navigation(&newer, "a");
    assert!(newer.execution_count("owner_tir(") > owners);
    assert!(newer.execution_count("symbol_signature(") > signatures);
    assert!(
        older
            .input_text(const_uri.as_str())
            .expect("old const")
            .contains("nat = 4")
    );
    assert!(
        newer
            .input_text(const_uri.as_str())
            .expect("new const")
            .contains("nat = 8")
    );
}

#[test]
fn older_snapshot_answers_from_its_own_inputs() {
    let dir = TempTree::new();
    let path = dir.path.join("only.syl");
    fs::write(&path, "cell Top(y: out Bit) { y := 1 }\n").unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(dir.path.clone()));
    host.set_roots(vec![path.clone()]);
    let older = host.snapshot().expect("older snapshot");
    let uri = older.files()[0].uri().clone();
    let source_id = older.files()[0].source_id();
    assert!(older.input_text(uri.as_str()).unwrap().contains("y := 1"));
    assert!(
        older
            .source_map()
            .file(source_id)
            .unwrap()
            .text()
            .contains("y := 1")
    );

    fs::write(&path, "cell Top(y: out Bit) { y := 0 }\n").unwrap();
    let newer = host.snapshot().expect("newer snapshot");
    let newer_file = newer.file_by_uri(&uri).expect("newer file");
    assert!(older.input_text(uri.as_str()).unwrap().contains("y := 1"));
    assert!(
        older
            .source_map()
            .file(source_id)
            .unwrap()
            .text()
            .contains("y := 1")
    );
    assert!(newer.input_text(uri.as_str()).unwrap().contains("y := 0"));
    assert!(
        newer
            .source_map()
            .file(newer_file.source_id())
            .unwrap()
            .text()
            .contains("y := 0")
    );
}

#[test]
fn disk_import_root_config_std_and_documents_refresh_inputs() {
    let dir = TempTree::new();
    let app = dir.path.join("app.syl");
    let lib = dir.path.join("lib.syl");
    let extra = dir.path.join("extra.syl");
    fs::write(&app, "cell Top(y: out Bit) { y := 1 }\n").unwrap();
    fs::write(&lib, "fn helper() -> nat { return 1 }\n").unwrap();
    fs::write(&extra, "fn extra() -> nat { return 1 }\n").unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(dir.path.clone()));
    host.set_roots(vec![app.clone()]);
    let alone = host.snapshot().expect("app without imports");
    assert_eq!(alone.files().len(), 1);

    fs::write(&app, "use lib.helper\ncell Top(y: out Bit) { y := 1 }\n").unwrap();
    let imported = host.snapshot().expect("import added");
    assert_eq!(imported.files().len(), 2, "adding a use loads the target");
    assert!(has_module(&imported, "lib"));

    fs::write(&app, "use extra.extra\ncell Top(y: out Bit) { y := 1 }\n").unwrap();
    let renamed = host.snapshot().expect("import renamed");
    assert!(has_module(&renamed, "extra"));
    assert!(!has_module(&renamed, "lib"));
    assert_eq!(imported.files().len(), 2);
    assert!(has_module(&imported, "lib"));

    fs::write(&app, "cell Top(y: out Bit) { y := 1 }\n").unwrap();
    let deleted = host.snapshot().expect("import deleted");
    assert_eq!(deleted.files().len(), 1);
    assert_eq!(renamed.files().len(), 2);

    let other = dir.path.join("other.syl");
    fs::write(&other, "cell Other(y: out Bit) { y := 0 }\n").unwrap();
    let rooted = deleted;
    host.set_roots(vec![other.clone()]);
    let rerooted = host.snapshot().expect("roots replaced");
    assert!(has_module(&rerooted, "other"));
    assert!(!has_module(&rerooted, "app"));
    assert!(has_module(&rooted, "app"));

    let nested = dir.path.join("nested");
    fs::create_dir_all(&nested).unwrap();
    let gate = nested.join("gate.syl");
    fs::write(&gate, "cell Gate(y: out Bit) { y := 1 }\n").unwrap();
    let configured =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(dir.path.clone()));
    let mut configured = configured;
    configured.set_roots(vec![gate.clone()]);
    let with_root = configured.snapshot().expect("workspace root snapshot");
    assert_eq!(
        with_root.files()[0].module_path(),
        &["nested".to_string(), "gate".to_string()]
    );
    let mut bare = AnalysisHost::new();
    bare.set_roots(vec![gate.clone()]);
    let without_root = bare.snapshot().expect("unconfigured snapshot");
    assert_eq!(without_root.files()[0].module_path(), &["gate".to_string()]);

    let std_root = dir.path.join("library");
    fs::create_dir_all(&std_root).unwrap();
    let bit = std_root.join("bit.syl");
    fs::write(&bit, "extern cell BitBox(y: out Bit)\n").unwrap();
    let mut std_host =
        AnalysisHost::with_config(ProjectConfig::new().with_std_root(std_root.clone()));
    std_host.set_roots(vec![bit]);
    let std_snapshot = std_host.snapshot().expect("std root snapshot");
    assert_eq!(
        std_snapshot.files()[0].module_path(),
        &["std".to_string(), "bit".to_string()]
    );

    let uri = DocumentUri::from_file_path(&dir.path.join("note.syl"));
    let mut documents = AnalysisHost::new();
    documents.open_document(
        uri.clone(),
        "cell Note(y: out Bit) { y := 1 }\n".to_string(),
        DocumentVersion::new(1),
    );
    let opened = documents.snapshot().expect("opened document");
    assert!(opened.input_text(uri.as_str()).unwrap().contains("y := 1"));
    documents
        .update_document_at_version(
            &uri,
            "cell Note(y: out Bit) { y := 0 }\n".to_string(),
            DocumentVersion::new(2),
        )
        .unwrap();
    let updated = documents.snapshot().expect("updated document");
    assert!(opened.input_text(uri.as_str()).unwrap().contains("y := 1"));
    assert!(updated.input_text(uri.as_str()).unwrap().contains("y := 0"));
    let stale = documents.update_document_at_version(
        &uri,
        "cell Note(y: out Bit) { y := 1 }\n".to_string(),
        DocumentVersion::new(2),
    );
    assert!(stale.is_err());
    assert!(updated.input_text(uri.as_str()).unwrap().contains("y := 0"));
    assert!(documents.close_document(&uri).is_some());
    let closed = documents.snapshot().expect("closed document");
    assert!(closed.file_by_uri(&uri).is_none());
    assert!(updated.file_by_uri(&uri).is_some());
}

#[test]
fn opaque_overlay_refreshes_new_snapshot_only() {
    let dir = TempTree::new();
    let path = dir.path.join("box.syl");
    fs::write(&path, "extern cell VendorBox(y: in Bit)\n").unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(dir.path.clone()));
    host.set_roots(vec![path]);
    let older = host.snapshot().expect("before overlay");
    let before = older
        .opaque_summaries()
        .and_then(|table| table.get("VendorBox"))
        .cloned();
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
    let older_summary = older
        .opaque_summaries()
        .and_then(|table| table.get("VendorBox"));
    let newer_summary = newer
        .opaque_summaries()
        .and_then(|table| table.get("VendorBox"))
        .expect("overlay is visible on the new snapshot");
    assert_eq!(older_summary, before.as_ref());
    assert!(matches!(
        newer_summary.trust_boundary(),
        TrustBoundary::VendorBlackBox { vendor } if vendor == "acme"
    ));
}

#[test]
fn unchanged_inputs_reuse_elaboration() {
    let mut host = AnalysisHost::new();
    host.open_document(
        DocumentUri::new("untitled:syl/cell"),
        "cell Top(y: out Bit) { y := 1 }\n".to_string(),
        DocumentVersion::new(1),
    );
    let first = host.snapshot().expect("first snapshot");
    first
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("first elaboration");
    let executions = first.execution_count("workspace_elaboration(");
    let second = host.snapshot().expect("unchanged snapshot");
    second
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("second elaboration");
    assert_eq!(second.execution_count("workspace_elaboration("), executions);
}

#[test]
fn ide_queries_do_not_elaborate() {
    let mut host = AnalysisHost::new();
    let uri = DocumentUri::new("untitled:syl/ide");
    // Second drive is diagnosed only by elaboration. HIR and TIR stay clean,
    // which is when grouped diagnostics used to build HWIR.
    host.open_document(
        uri.clone(),
        "cell Top(y: out Bit) {\n    y := 0\n    y := 1\n}\n".to_string(),
        DocumentVersion::new(1),
    );
    let snapshot = host.snapshot().expect("ide snapshot");
    let workspace_elaboration = snapshot.execution_count("workspace_elaboration(");
    let unit_elaboration = snapshot.execution_count("unit_elaboration(");
    let hardware = snapshot.execution_count("hardware_contract(");
    let const_mir = snapshot.execution_count("owner_const_mir(");
    let map_ir = snapshot.execution_count("owner_map_ir(");
    let module_eir = snapshot.execution_count("owner_module_eir(");
    let token = CancellationToken::new();
    snapshot
        .hir_analysis_for_uri_with_token(&uri, &token)
        .expect("definition and completion HIR")
        .expect("definition and completion HIR result");
    snapshot
        .tir_analysis_for_uri_with_token(&uri, &token)
        .expect("hover TIR")
        .expect("hover TIR result");
    let _symbols = snapshot
        .file_by_uri(&uri)
        .expect("document symbol file")
        .ast();
    let staged = snapshot
        .package_stage_diagnostics_with_token(&uri, &token)
        .expect("IDE publication diagnostics")
        .expect("package stages");
    assert!(staged.elaboration().is_empty());
    assert!(
        staged
            .parse()
            .iter()
            .chain(staged.hir())
            .chain(staged.tir())
            .chain(staged.elaboration())
            .all(|diagnostic| {
                diagnostic.code.as_deref() != Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
            }),
        "hover, definition, completion, symbols, and IDE diagnostics must not publish HWIR"
    );
    assert!(snapshot.execution_count("owner_hir(") >= 1);
    assert!(snapshot.execution_count("owner_tir(") >= 1);
    assert!(snapshot.execution_count("ide_unit_hir(") >= 1);
    assert!(snapshot.execution_count("ide_unit_tir(") >= 1);
    assert!(snapshot.execution_count("file_item_index(") >= 1);
    assert_eq!(
        snapshot.execution_count("workspace_elaboration("),
        workspace_elaboration
    );
    assert_eq!(
        snapshot.execution_count("unit_elaboration("),
        unit_elaboration
    );
    assert_eq!(snapshot.execution_count("hardware_contract("), hardware);
    assert_eq!(snapshot.execution_count("owner_const_mir("), const_mir);
    assert_eq!(snapshot.execution_count("owner_map_ir("), map_ir);
    assert_eq!(snapshot.execution_count("owner_module_eir("), module_eir);
    assert_eq!(hardware, 0);
    assert_eq!(const_mir, 0);
    assert_eq!(map_ir, 0);
    assert_eq!(module_eir, 0);
    let cli = snapshot.semantic_diagnostics();
    assert!(
        cli.iter().any(|diagnostic| {
            diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
        }),
        "CLI semantic diagnostics still report the duplicate driver"
    );
    assert!(snapshot.execution_count("workspace_elaboration(") > workspace_elaboration);
    assert!(snapshot.execution_count("hardware_contract(") > hardware);
    assert!(snapshot.execution_count("owner_const_mir(") > const_mir);
    assert!(snapshot.execution_count("owner_map_ir(") > map_ir);
    assert!(snapshot.execution_count("owner_module_eir(") > module_eir);
}

#[test]
fn cancelled_elaboration_is_not_stored_and_retry_completes() {
    let snapshot = cell_snapshot();
    let token = CancellationToken::new();
    token.cancel();
    let err = snapshot
        .elaboration_diagnostics_with_token(&token)
        .expect_err("cancelled elaboration");
    assert!(matches!(err, ProjectError::Cancelled));
    let started = snapshot.execution_count("workspace_elaboration(");
    assert!(started >= 1);
    snapshot
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("retry returns a complete elaboration");
    let completed = snapshot.execution_count("workspace_elaboration(");
    assert!(completed > started);
    snapshot
        .elaboration_diagnostics_with_token(&CancellationToken::new())
        .expect("memo hit");
    assert_eq!(
        snapshot.execution_count("workspace_elaboration("),
        completed
    );
}

#[test]
fn cancelling_one_request_does_not_stop_another() {
    let snapshot = cell_snapshot();
    let other = snapshot.clone();
    let gate = Arc::new(Barrier::new(2));
    let other_gate = Arc::clone(&gate);
    let worker = thread::spawn(move || {
        other_gate.wait();
        other
            .hir_analysis_with_token(&CancellationToken::new())
            .map(|_| ())
    });
    let token = CancellationToken::new();
    token.cancel();
    gate.wait();
    let cancelled = snapshot.hir_analysis_with_token(&token);
    let other_result = worker.join().expect("other request thread");
    assert!(other_result.is_ok(), "the other request must finish");
    if cancelled.is_err() {
        snapshot
            .hir_analysis_with_token(&CancellationToken::new())
            .expect("retry after cancel");
    }
}

#[test]
fn already_cancelled_snapshot_returns_before_query() {
    let mut host = AnalysisHost::new();
    host.open_document(
        DocumentUri::new("untitled:syl/cancel-snapshot"),
        "cell Top(y: out Bit) { y := 1 }\n".to_string(),
        DocumentVersion::new(1),
    );
    let token = CancellationToken::new();
    token.cancel();
    let err = host
        .snapshot_with_token(&token)
        .expect_err("pre-cancelled snapshot");
    assert!(matches!(err, ProjectError::Cancelled));
}

fn cell_snapshot() -> AnalysisSnapshot {
    let mut host = AnalysisHost::new();
    host.open_document(
        DocumentUri::new("untitled:syl/cell"),
        "cell Top(y: out Bit) { y := 1 }\n".to_string(),
        DocumentVersion::new(1),
    );
    host.snapshot().expect("cell snapshot")
}

fn span_covering(snapshot: &AnalysisSnapshot, uri: &DocumentUri, needle: &str) -> syl_span::Span {
    let file = snapshot.file_by_uri(uri).expect("file");
    let text = snapshot.input_text(uri.as_str()).expect("text");
    let start = text
        .find(needle)
        .unwrap_or_else(|| panic!("missing {needle} in {text}"));
    syl_span::Span::new_in(file.source_id(), start, start + needle.len())
}

fn uri_for_module(snapshot: &AnalysisSnapshot, name: &str) -> DocumentUri {
    snapshot
        .files()
        .iter()
        .find(|file| file.module_path() == [name.to_string()])
        .unwrap_or_else(|| panic!("missing module {name}"))
        .uri()
        .clone()
}

fn read_owner_through_navigation(snapshot: &AnalysisSnapshot, module: &str) {
    let uri = uri_for_module(snapshot, module);
    let token = CancellationToken::new();
    snapshot
        .hir_analysis_for_uri_with_token(&uri, &token)
        .expect("definition and completion query")
        .expect("definition and completion HIR");
    snapshot
        .tir_analysis_for_uri_with_token(&uri, &token)
        .expect("hover query")
        .expect("hover TIR");
    let _symbols = snapshot.file_by_uri(&uri).expect("document symbols").ast();
    let staged = snapshot
        .package_stage_diagnostics_with_token(&uri, &token)
        .expect("IDE diagnostics")
        .expect("package stages");
    assert!(staged.elaboration().is_empty());
}

fn has_module(snapshot: &AnalysisSnapshot, name: &str) -> bool {
    snapshot
        .files()
        .iter()
        .any(|file| file.module_path() == [name.to_string()])
}

struct PairFixture {
    root: TempTree,
    lib_name: &'static str,
}

impl PairFixture {
    fn new(lib_source: &str, app_source: &str) -> Self {
        let root = TempTree::new();
        let lib_name = if lib_source.starts_with("cell Buf") {
            "buf"
        } else {
            "lib"
        };
        fs::write(root.path.join(format!("{lib_name}.syl")), lib_source).unwrap();
        fs::write(root.path.join("app.syl"), app_source).unwrap();
        Self { root, lib_name }
    }

    fn host(&self) -> AnalysisHost {
        let mut host = AnalysisHost::with_config(
            ProjectConfig::new().with_workspace_root(self.root.path.clone()),
        );
        host.set_roots(vec![self.root.path.join("app.syl")]);
        host
    }

    fn write_lib(&self, source: &str) {
        fs::write(
            self.root.path.join(format!("{}.syl", self.lib_name)),
            source,
        )
        .unwrap();
    }
}

struct TempTree {
    path: PathBuf,
}

impl TempTree {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "syl-cutover-{}-{}",
            std::process::id(),
            unique_suffix()
        ));
        fs::create_dir_all(&path).unwrap();
        let path = path.canonicalize().unwrap_or(path);
        Self { path }
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

fn unique_suffix() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}
