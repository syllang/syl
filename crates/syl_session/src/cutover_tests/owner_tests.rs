//! Owner-grain reuse: multi-decl aggregates, unused signatures, and import cycles.

use std::fs;

use super::{PairFixture, TempTree, read_owner_through_navigation, span_covering, uri_for_module};
use crate::{AnalysisHost, CancellationToken, ProjectConfig};

#[test]
fn two_local_decls_recompute_only_the_edited_owner() {
    let dir = TempTree::new();
    let path = dir.path.join("pair.syl");
    fs::write(
        &path,
        "cell Alpha(y: out Bit) { y := 1 }\ncell Beta(z: out Bit) { z := 0 }\n",
    )
    .unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(dir.path.clone()));
    host.set_roots(vec![path.clone()]);
    let older = host.snapshot().expect("older snapshot");
    let uri = uri_for_module(&older, "pair");
    read_owner_through_navigation(&older, "pair");
    let owner_tir = older.execution_count("owner_tir(");
    let owner_hir = older.execution_count("owner_hir(");
    assert!(owner_tir >= 2, "each local declaration is an owner");
    let old_text = older.input_text(uri.as_str()).expect("old text");

    fs::write(
        &path,
        "cell Alpha(y: out Bit) { y := 1 }\ncell Beta(z: out Bit) { z := 3 }\n",
    )
    .unwrap();
    let newer = host.snapshot().expect("edited snapshot");
    read_owner_through_navigation(&newer, "pair");
    assert_eq!(newer.execution_count("owner_tir("), owner_tir + 1);
    assert_eq!(newer.execution_count("owner_hir("), owner_hir + 1);
    assert_eq!(
        older.input_text(uri.as_str()).as_deref(),
        Some(old_text.as_str())
    );
    assert!(older.input_text(uri.as_str()).unwrap().contains("z := 0"));

    let token = CancellationToken::new();
    let hir = newer
        .hir_analysis_for_uri_with_token(&uri, &token)
        .expect("hir query")
        .expect("pair hir");
    let tir = newer
        .tir_analysis_for_uri_with_token(&uri, &token)
        .expect("tir query")
        .expect("pair tir");
    assert!(
        hir.definition_at(span_covering(&newer, &uri, "Alpha"))
            .is_some()
    );
    assert!(
        hir.definition_at(span_covering(&newer, &uri, "Beta"))
            .is_some()
    );
    assert!(tir.hover_at(span_covering(&newer, &uri, "1")).is_some());
    assert!(tir.hover_at(span_covering(&newer, &uri, "3")).is_some());
    let staged = newer
        .package_stage_diagnostics_with_token(&uri, &token)
        .expect("stages")
        .expect("pair stages");
    let codes = staged
        .hir()
        .iter()
        .chain(staged.tir())
        .filter_map(|diagnostic| diagnostic.code.as_deref())
        .collect::<Vec<_>>();
    assert!(
        !codes.contains(&"E_MIDDLE_DUPLICATE_FN"),
        "aggregate keeps one def per symbol: {codes:?}"
    );
}

#[test]
fn unused_signature_edit_reuses_the_importing_owner() {
    let fixture = PairFixture::new(
        "fn helper() -> nat { return 1 }\nfn unused() -> nat { return 0 }\n",
        "use lib.helper\ncell Top(y: out Bit) {\n    let n = helper()\n    y := 1\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    assert!(owners >= 1, "hover must execute the importing owner");
    let lib_uri = uri_for_module(&older, fixture.lib_name);
    let old_text = older.input_text(lib_uri.as_str()).expect("old lib text");

    fixture.write_lib("fn helper() -> nat { return 1 }\nfn unused(x: nat) -> nat { return 0 }\n");
    let newer = host.snapshot().expect("unused signature snapshot");
    read_owner_through_navigation(&newer, "app");
    assert_eq!(newer.execution_count("owner_tir("), owners);
    assert_eq!(
        older.input_text(lib_uri.as_str()).as_deref(),
        Some(old_text.as_str())
    );
    assert!(
        newer
            .input_text(lib_uri.as_str())
            .expect("new lib text")
            .contains("fn unused(x: nat)")
    );
}

#[test]
fn signature_closure_ignores_unnamed_items() {
    let original = "\
struct Point { data: Bit }
fn helper(p: Point) -> Point { return p }
const WIDTH: nat = 1
struct Unused { data: Bit }
use other.unused_name
";
    let fixture = PairFixture::new(
        original,
        "use lib.Point\nuse lib.helper\nfn run(p: Point) -> Point { return helper(p: p) }\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let uri = uri_for_module(&older, "app");
    let token = CancellationToken::new();
    let codes = stage_codes(&older, &uri, &token);
    assert!(
        !codes.iter().any(|code| code == "E_MIDDLE_UNKNOWN_IMPORT"),
        "helper and Point stay imported: {codes:?}"
    );
    assert!(
        !codes.iter().any(|code| code == "E_MIDDLE_UNRESOLVED_NAME"),
        "helper and Point resolve: {codes:?}"
    );

    fixture.write_lib(&original.replace("const WIDTH: nat = 1", "const WIDTH: nat = 2"));
    let width = host.snapshot().expect("width snapshot");
    read_owner_through_navigation(&width, "app");
    assert_eq!(width.execution_count("owner_tir("), owners);

    fixture.write_lib(&original.replace(
        "struct Unused { data: Bit }",
        "struct Unused { data: Bit, extra: Bit }",
    ));
    let unused_struct = host.snapshot().expect("unused struct snapshot");
    read_owner_through_navigation(&unused_struct, "app");
    assert_eq!(unused_struct.execution_count("owner_tir("), owners);

    fixture.write_lib(&original.replace("use other.unused_name", "use other.renamed"));
    let unused_use = host.snapshot().expect("unused use snapshot");
    read_owner_through_navigation(&unused_use, "app");
    assert_eq!(unused_use.execution_count("owner_tir("), owners);

    fixture.write_lib(&original.replace(
        "struct Point { data: Bit }",
        "struct Point { data: Bit, tag: Bit }",
    ));
    let point = host.snapshot().expect("point snapshot");
    read_owner_through_navigation(&point, "app");
    assert!(point.execution_count("owner_tir(") > owners);
    assert_eq!(
        older
            .input_text(uri_for_module(&older, "lib").as_str())
            .as_deref(),
        Some(original)
    );
    assert!(
        older
            .input_text(uri_for_module(&older, "lib").as_str())
            .unwrap()
            .contains("nat = 1")
    );
}

#[test]
fn length_changing_edit_refreshes_definition_spans() {
    let original = "const WIDTH: nat = 1\nfn helper() -> nat { return 1 }\n";
    let fixture = PairFixture::new(
        original,
        "use lib.helper\ncell Top(y: out Bit) {\n    let n = helper()\n    y := 1\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("older snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let lib_uri = uri_for_module(&older, "lib");
    assert_definition_covers_helper(&older);

    fixture.write_lib(&original.replace("const WIDTH: nat = 1", "const WIDTH: nat = 10"));
    let newer = host.snapshot().expect("shifted snapshot");
    read_owner_through_navigation(&newer, "app");
    assert!(
        newer.execution_count("owner_tir(") > owners,
        "shifted definition spans must rebuild the importing owner"
    );
    assert_definition_covers_helper(&newer);
    assert_definition_covers_helper(&older);
    assert!(
        older
            .input_text(lib_uri.as_str())
            .expect("frozen lib")
            .contains("nat = 1")
    );
    assert!(
        newer
            .input_text(lib_uri.as_str())
            .expect("shifted lib")
            .contains("nat = 10")
    );
}

#[test]
fn field_type_reads_same_named_const() {
    // Same-length value edit. A longer literal would also move Packet's spans.
    let original = "const WIDTH: nat = 8\nstruct Packet { WIDTH: Bit, data: UInt<WIDTH> }\n";
    let edited = original.replace("const WIDTH: nat = 8", "const WIDTH: nat = 9");
    assert_const_width_reaches_type_user(
        original,
        &edited,
        "use lib.Packet\nfn run(p: Packet) -> Packet { return p }\n",
    );
}

#[test]
fn port_type_reads_same_named_const() {
    let original = "const WIDTH: nat = 8\ncell Pipe(WIDTH: in Bit, data: in UInt<WIDTH>) -> y: Bit { y := WIDTH }\n";
    let edited = original.replace("const WIDTH: nat = 8", "const WIDTH: nat = 9");
    assert_const_width_reaches_type_user(
        original,
        &edited,
        "use lib.Pipe\ncell Top(up: in Bit, y: out Bit) {\n    let placed = place Pipe(WIDTH: up, data: 0, y: y)\n}\n",
    );
}

#[test]
fn result_type_reads_same_named_const() {
    let original =
        "const WIDTH: nat = 8\ncell Pipe(data: in Bit) -> WIDTH: UInt<WIDTH> { WIDTH := 0 }\n";
    let edited = original.replace("const WIDTH: nat = 8", "const WIDTH: nat = 9");
    assert_const_width_reaches_type_user(
        original,
        &edited,
        "use lib.Pipe\ncell Top(up: in Bit, y: out UInt<8>) {\n    let placed = place Pipe(data: up, WIDTH: y)\n}\n",
    );
}

fn assert_const_width_reaches_type_user(original: &str, edited: &str, app: &str) {
    let fixture = PairFixture::new(original, app);
    let mut host = fixture.host();
    let older = host.snapshot().expect("older snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let app_uri = uri_for_module(&older, "app");
    let lib_uri = uri_for_module(&older, "lib");
    let token = CancellationToken::new();
    let hir = older
        .hir_analysis_for_uri_with_token(&app_uri, &token)
        .expect("hir query")
        .expect("app hir");
    let width = hir
        .completion_items()
        .into_iter()
        .find(|item| item.label() == "WIDTH" && item.kind() == syl_sema::CompletionKind::Const)
        .expect("UInt<WIDTH> must see const WIDTH");
    let lib_text = older.input_text(lib_uri.as_str()).expect("lib text");
    let decl = lib_text.find("const WIDTH").expect("const WIDTH");
    assert_eq!(width.span().start, decl);
    assert!(
        older
            .source_map()
            .file(width.span().source)
            .expect("const span source")
            .text()
            .contains("nat = 8")
    );
    let codes = stage_codes(&older, &app_uri, &token);
    assert!(
        !codes.iter().any(|code| {
            code == "E_MIDDLE_UNKNOWN_ELAB_IDENTIFIER" || code == "E_MIDDLE_UNRESOLVED_NAME"
        }),
        "WIDTH resolves in the type: {codes:?}"
    );

    fixture.write_lib(edited);
    let newer = host.snapshot().expect("width snapshot");
    read_owner_through_navigation(&newer, "app");
    assert!(
        newer.execution_count("owner_tir(") > owners,
        "editing const WIDTH must recompute the type user"
    );
    assert_eq!(
        older.input_text(lib_uri.as_str()).as_deref(),
        Some(original)
    );
}

#[test]
fn parameter_binder_does_not_read_same_named_const() {
    let original = "const x: nat = 1\nfn helper(x: nat) -> nat { return x }\n";
    let fixture = PairFixture::new(
        original,
        "use lib.helper\ncell Top(y: out Bit) {\n    let n = helper(0)\n    y := 1\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("older snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let lib_uri = uri_for_module(&older, "lib");
    assert!(owners >= 1, "hover must execute the importing owner");

    fixture.write_lib(&original.replace("const x: nat = 1", "const x: nat = 2"));
    let newer = host.snapshot().expect("const value snapshot");
    read_owner_through_navigation(&newer, "app");
    assert_eq!(newer.execution_count("owner_tir("), owners);
    assert_eq!(
        older.input_text(lib_uri.as_str()).as_deref(),
        Some(original)
    );

    fixture.write_lib(&original.replace("fn helper(x: nat)", "fn helper(x: Bit)"));
    let signature = host.snapshot().expect("signature snapshot");
    read_owner_through_navigation(&signature, "app");
    assert!(signature.execution_count("owner_tir(") > owners);
    assert!(
        older
            .input_text(lib_uri.as_str())
            .expect("frozen const")
            .contains("nat = 1")
    );
}

fn assert_definition_covers_helper(snapshot: &crate::AnalysisSnapshot) {
    let app_uri = uri_for_module(snapshot, "app");
    let token = CancellationToken::new();
    let hir = snapshot
        .hir_analysis_for_uri_with_token(&app_uri, &token)
        .expect("hir query")
        .expect("app hir");
    let defined = hir
        .definition_at(span_covering(snapshot, &app_uri, "helper"))
        .expect("helper definition");
    assert_eq!(defined.name(), "helper");
    let span = defined.span();
    let text = snapshot
        .source_map()
        .file(span.source)
        .expect("definition span resolves in this snapshot source map")
        .text();
    assert!(
        span.end <= text.len(),
        "span {span:?} exceeds the source map"
    );
    let slice = &text[span.start..span.end];
    assert!(
        slice.contains("helper"),
        "definition span {span:?} slice {slice:?} does not cover helper"
    );
    let declaration = text
        .find("fn helper")
        .expect("fn helper in the definition file");
    assert_eq!(
        span.start, declaration,
        "definition span must start at fn helper in this source map: {text:?}"
    );
}

fn stage_codes(
    snapshot: &crate::AnalysisSnapshot,
    uri: &crate::DocumentUri,
    token: &CancellationToken,
) -> Vec<String> {
    let staged = snapshot
        .package_stage_diagnostics_with_token(uri, token)
        .expect("stages")
        .expect("app stages");
    staged
        .hir()
        .iter()
        .chain(staged.tir())
        .filter_map(|diagnostic| diagnostic.code.clone())
        .collect()
}

#[test]
fn second_named_import_from_the_same_module_resolves() {
    let fixture = PairFixture::new(
        "enum Op { Add, Xor }\ncell skid(up: in Bit) -> down: Bit { down := up }\ncell unused(up: in Bit) -> down: Bit { down := up }\n",
        "use lib.Op\nuse lib.skid\ncell Top(up: in Bit, y: out Bit) {\n    let placed = place skid(up: up, down: y)\n}\n",
    );
    let mut host = fixture.host();
    let older = host.snapshot().expect("initial snapshot");
    read_owner_through_navigation(&older, "app");
    let owners = older.execution_count("owner_tir(");
    let uri = uri_for_module(&older, "app");
    let token = CancellationToken::new();
    let staged = older
        .package_stage_diagnostics_with_token(&uri, &token)
        .expect("stages")
        .expect("app stages");
    let codes = staged
        .hir()
        .iter()
        .chain(staged.tir())
        .filter_map(|diagnostic| diagnostic.code.as_deref())
        .collect::<Vec<_>>();
    assert!(
        !codes.contains(&"E_MIDDLE_UNKNOWN_IMPORT"),
        "named cell import must resolve beside the enum: {codes:?}"
    );
    assert!(
        !codes.contains(&"E_MIDDLE_UNRESOLVED_NAME"),
        "named cell must stay visible: {codes:?}"
    );

    fixture.write_lib(
        "enum Op { Add, Xor }\ncell skid(up: in Bit) -> down: Bit { down := up }\ncell unused(up: in Bit, extra: in Bit) -> down: Bit { down := up }\n",
    );
    let newer = host.snapshot().expect("unused cell signature snapshot");
    read_owner_through_navigation(&newer, "app");
    assert_eq!(newer.execution_count("owner_tir("), owners);
}

#[test]
fn import_cycle_adds_no_duplicate_declaration() {
    let root = TempTree::new();
    fs::write(
        root.path.join("a.syl"),
        "use b.beta\nfn alpha() -> nat { return 1 }\n",
    )
    .unwrap();
    fs::write(
        root.path.join("b.syl"),
        "use a.alpha\nfn beta() -> nat { return 2 }\n",
    )
    .unwrap();
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(root.path.clone()));
    host.set_roots(vec![root.path.join("a.syl")]);
    let snapshot = host.snapshot().expect("cycle snapshot");
    let uri = uri_for_module(&snapshot, "a");
    let token = CancellationToken::new();
    let hir = snapshot
        .hir_analysis_for_uri_with_token(&uri, &token)
        .expect("hir query")
        .expect("cycle hir");
    assert!(
        hir.definition_at(span_covering(&snapshot, &uri, "alpha"))
            .is_some()
    );
    let staged = snapshot
        .package_stage_diagnostics_with_token(&uri, &token)
        .expect("stages")
        .expect("cycle stages");
    let codes = staged
        .hir()
        .iter()
        .chain(staged.tir())
        .filter_map(|diagnostic| diagnostic.code.as_deref())
        .collect::<Vec<_>>();
    assert!(
        !codes.contains(&"E_MIDDLE_DUPLICATE_FN"),
        "import cycle must not duplicate the owner: {codes:?}"
    );
    assert!(
        !codes.contains(&"E_MIDDLE_DUPLICATE_CONST"),
        "import cycle must not duplicate a const: {codes:?}"
    );
}
