use super::*;
use crate::{SylLanguageServer, protocol_tests::TestClient};
use serde_json::json;
use std::{
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
use syl_query::AnalysisQueries;
use syl_session::DocumentVersion;
use tower_lsp::{LanguageServer, LspService, lsp_types::*};

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "syl-lsp-workspace-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, path: &str, text: &str) -> PathBuf {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    fn uri(&self, path: &str) -> Url {
        Url::from_file_path(self.0.join(path)).unwrap()
    }
    fn event(&self, path: &str, typ: FileChangeType) -> FileEvent {
        FileEvent {
            uri: self.uri(path),
            typ,
        }
    }
    fn params(&self) -> InitializeParams {
        InitializeParams {
            root_uri: Some(self.uri("")),
            ..Default::default()
        }
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest() -> &'static str {
    "[paths]\nworkspace_roots=['src']\nstd_roots=['std']\npackage_roots=['pkg']\nopaque_summaries=['meta.json']\n"
}
fn summary() -> &'static str {
    r#"{"schema_version":1,"summaries":[{"kind":"precompiled_cell","callable":"VendorLatch","endpoints":[{"name":"y","direction":"in","layout":"bit","capability":"value"}],"driven_fields":["y"],"latency_class":"sequential","trust_boundary":"trusted_precompiled"}]}"#
}

#[test]
fn manifest_discovers_parent_resolves_all_roots_and_loads_summaries() {
    let tree = Tree::new();
    tree.write("syl.toml", manifest());
    tree.write("meta.json", summary());
    tree.write("std/base.syl", "const WIDTH: nat = 8\n");
    tree.write("pkg/dep.syl", "cell Helper(y: out Bit) { y := 0 }\n");
    tree.write("src/main.syl", "use std.base.WIDTH\nuse dep.Helper\nextern cell VendorLatch(y: in Bit)\ncell Top(y: out Bit) {\n signal tmp: Bit := 0\n let v = place VendorLatch(y: tmp)\n y := tmp\n}\n");
    let mut params = tree.params();
    params.root_uri = Some(tree.uri("src"));
    let mut workspace = WorkspaceState::new(&params);
    let mut host = AnalysisHost::new();
    workspace.configure_host(&mut host).unwrap();
    assert_eq!(workspace.manifest_path, Some(tree.0.join("syl.toml")));
    assert_eq!(host.roots(), &[tree.0.join("src/main.syl")]);
    let snapshot = host.snapshot().unwrap();
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let fast = snapshot.grouped_diagnostics();
    assert!(
        fast.packages()
            .iter()
            .flat_map(|p| p.documents())
            .all(|d| d.diagnostics().is_empty()),
        "{fast:?}"
    );
    let drc = snapshot.background_elaboration_diagnostics();
    assert!(
        drc.packages()
            .iter()
            .flat_map(|p| p.documents())
            .flat_map(|d| d.diagnostics())
            .any(|d| d.code() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
    );
}

#[tokio::test]
async fn malformed_manifest_initialization_records_error_and_fix_preserves_overlay() {
    let tree = Tree::new();
    tree.write("syl.toml", "[broken");
    tree.write("main.syl", "cell Disk() {}\n");
    let (service, _socket) = LspService::new(SylLanguageServer::new);
    let server = service.inner();
    server.initialize(tree.params()).await.unwrap();
    assert!(server.initialization_error.lock().await.is_some());
    assert_eq!(
        server.workspace.lock().await.error_uri(),
        Some(tree.uri("syl.toml"))
    );
    let uri = DocumentUri::new(tree.uri("main.syl").as_str());
    server.host.lock().await.open_document(
        uri.clone(),
        "cell Unsaved() {}\n".into(),
        DocumentVersion::new(8),
    );
    tree.write("syl.toml", "[paths]\nworkspace_roots=['.']\n");
    server
        .did_change_watched_files(DidChangeWatchedFilesParams {
            changes: vec![tree.event("syl.toml", FileChangeType::CHANGED)],
        })
        .await;
    let host = server.host.lock().await;
    assert_eq!(host.overlay(&uri).unwrap().text(), "cell Unsaved() {}\n");
    assert_eq!(
        host.overlay(&uri).unwrap().version(),
        DocumentVersion::new(8)
    );
    server.next_diagnostics_generation();
}

#[test]
fn manifest_free_configuration_resolves_imports_and_workspace_folders_take_precedence() {
    let tree = Tree::new();
    tree.write("lib.syl", "cell Helper(y: out Bit) { y := 0 }\n");
    tree.write("app.syl", "use lib.Helper\ncell Top() {}\n");
    let mut params = tree.params();
    params.root_uri = Some(Url::parse("file:///does-not-exist").unwrap());
    params.workspace_folders = Some(vec![WorkspaceFolder {
        uri: tree.uri(""),
        name: "workspace".into(),
    }]);
    let mut workspace = WorkspaceState::new(&params);
    let mut host = AnalysisHost::new();
    workspace.configure_host(&mut host).unwrap();
    assert_eq!(workspace.roots, std::slice::from_ref(&tree.0));
    let snapshot = host.snapshot().unwrap();
    assert!(snapshot.semantic_diagnostics().is_empty());
}

#[test]
fn watched_files_reload_metadata_refresh_roots_and_preserve_unsaved_documents() {
    let tree = Tree::new();
    tree.write("syl.toml", manifest());
    tree.write("meta.json", summary());
    tree.write("src/main.syl", "cell Disk() {}\n");
    let mut workspace = WorkspaceState::new(&tree.params());
    let mut host = AnalysisHost::new();
    workspace.configure_host(&mut host).unwrap();
    let uri = DocumentUri::new(tree.uri("src/main.syl").as_str());
    host.open_document(
        uri.clone(),
        "cell Unsaved() {}\n".into(),
        DocumentVersion::new(7),
    );
    assert_eq!(
        workspace.classify_file_events(
            &[tree.event("src/main.syl", FileChangeType::CHANGED)],
            &host
        ),
        WatchAction::Ignore
    );
    assert_eq!(
        workspace.classify_file_events(&[tree.event("other.txt", FileChangeType::CHANGED)], &host),
        WatchAction::Ignore
    );
    tree.write("src/new.syl", "cell Added() {}\n");
    let action = workspace
        .classify_file_events(&[tree.event("src/new.syl", FileChangeType::CREATED)], &host);
    assert_eq!(
        action,
        WatchAction::RefreshSylFiles {
            structural_change: true
        }
    );
    workspace.apply_watch_action(&mut host, action).unwrap();
    assert_eq!(host.roots().len(), 2);
    let disk_uri = DocumentUri::new(tree.uri("src/new.syl").as_str());
    tree.write("src/new.syl", "cell Changed() {}\n");
    let action = workspace
        .classify_file_events(&[tree.event("src/new.syl", FileChangeType::CHANGED)], &host);
    assert_eq!(
        action,
        WatchAction::RefreshSylFiles {
            structural_change: false
        }
    );
    workspace.apply_watch_action(&mut host, action).unwrap();
    let snapshot = host.snapshot().unwrap();
    let file = snapshot.file_by_uri(&disk_uri).unwrap();
    assert!(
        snapshot
            .source_map()
            .file(file.source_id())
            .unwrap()
            .text()
            .contains("Changed")
    );
    fs::remove_file(tree.0.join("src/new.syl")).unwrap();
    let action = workspace
        .classify_file_events(&[tree.event("src/new.syl", FileChangeType::DELETED)], &host);
    workspace.apply_watch_action(&mut host, action).unwrap();
    assert_eq!(host.roots().len(), 1);
    tree.write("meta.json", "invalid");
    let action =
        workspace.classify_file_events(&[tree.event("meta.json", FileChangeType::CHANGED)], &host);
    assert_eq!(action, WatchAction::ReloadOpaqueSummaries);
    assert!(workspace.apply_watch_action(&mut host, action).is_err());
    assert!(host.opaque_summaries().get("VendorLatch").is_some());
    tree.write("meta.json", "{\"schema_version\":1,\"summaries\":[]}");
    workspace
        .apply_watch_action(&mut host, WatchAction::ReloadOpaqueSummaries)
        .unwrap();
    assert!(host.opaque_summaries().get("VendorLatch").is_none());
    tree.write("meta.json", summary());
    workspace
        .apply_watch_action(&mut host, WatchAction::ReloadOpaqueSummaries)
        .unwrap();
    assert!(host.opaque_summaries().get("VendorLatch").is_some());
    fs::remove_file(tree.0.join("syl.toml")).unwrap();
    workspace
        .apply_watch_action(&mut host, WatchAction::ReloadManifest)
        .unwrap();
    assert!(workspace.manifest_path.is_none());
    assert!(host.opaque_summaries().get("VendorLatch").is_none());
    assert_eq!(host.overlay(&uri).unwrap().text(), "cell Unsaved() {}\n");
    assert_eq!(
        host.overlay(&uri).unwrap().version(),
        DocumentVersion::new(7)
    );
}

#[tokio::test(start_paused = true)]
async fn watcher_registration_rejection_falls_back_and_changed_paths_reregister() {
    let tree = Tree::new();
    tree.write("syl.toml", manifest());
    tree.write("meta.json", summary());
    tree.write("src/main.syl", "cell Top() {}\n");
    let mut client = TestClient::new();
    client.initialize(json!({"rootUri":tree.uri(""), "capabilities":{"workspace":{"didChangeWatchedFiles":{"dynamicRegistration":true}}}})).await;
    let registration = client.recv().await;
    assert_eq!(registration["method"], "client/registerCapability");
    let watchers = registration["params"]["registrations"][0]["registerOptions"]["watchers"]
        .as_array()
        .unwrap();
    assert!(watchers.iter().any(|w| w["globPattern"] == "**/*.syl"));
    assert!(
        watchers
            .iter()
            .any(|w| w["globPattern"] == tree.0.join("meta.json").to_str().unwrap())
    );
    client.send(json!({"jsonrpc":"2.0", "id":registration["id"], "error":{"code":-32601,"message":"unsupported"}})).await;
    let log = client.recv().await;
    assert_eq!(log["method"], "window/logMessage");
    let initial = client.recv().await;
    assert_eq!(initial["method"], "textDocument/publishDiagnostics");
    tree.write("next.toml", "schema_version=1\nsummaries=[]\n");
    tree.write(
        "syl.toml",
        "[paths]\nworkspace_roots=['src']\nopaque_summaries=['next.toml']\n",
    );
    client.send(json!({"jsonrpc":"2.0", "method":"workspace/didChangeWatchedFiles", "params":{"changes":[{"uri":tree.uri("syl.toml"),"type":2}]}})).await;
    let second = client.recv().await;
    assert_eq!(second["method"], "client/registerCapability");
    assert!(
        second["params"]["registrations"][0]["registerOptions"]["watchers"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["globPattern"] == tree.0.join("next.toml").to_str().unwrap())
    );
    client
        .send(json!({"jsonrpc":"2.0", "id":second["id"], "result":null}))
        .await;
    client.settle().await;
    tokio::time::advance(std::time::Duration::from_millis(150)).await;
    assert_eq!(
        client.recv().await["method"],
        "textDocument/publishDiagnostics"
    );
    // A successful registration must be explicitly replaced on the next edit.
    tree.write("syl.toml", "[paths]\nworkspace_roots=['src']\n");
    client.send(json!({"jsonrpc":"2.0", "method":"workspace/didChangeWatchedFiles", "params":{"changes":[{"uri":tree.uri("syl.toml"),"type":2}]}})).await;
    let unregister = client.recv().await;
    assert_eq!(unregister["method"], "client/unregisterCapability");
    client
        .send(json!({"jsonrpc":"2.0","id":unregister["id"],"result":null}))
        .await;
    let register = client.recv().await;
    assert_eq!(register["method"], "client/registerCapability");
    client
        .send(json!({"jsonrpc":"2.0","id":register["id"],"result":null}))
        .await;
}

#[tokio::test(start_paused = true)]
async fn wire_manifest_error_clears_after_static_file_notification_recovers() {
    let tree = Tree::new();
    tree.write("syl.toml", "[broken");
    tree.write("main.syl", "cell Top() {}\n");
    let mut client = TestClient::new();
    client
        .initialize(json!({"rootUri":tree.uri(""), "capabilities":{}}))
        .await;
    let error = client.recv().await;
    assert_eq!(error["params"]["uri"], tree.uri("syl.toml").as_str());
    assert_eq!(error["params"]["diagnostics"][0]["code"], "E_PROJECT");
    tree.write("syl.toml", "[paths]\nworkspace_roots=['.']\n");
    client.send(json!({"jsonrpc":"2.0", "method":"workspace/didChangeWatchedFiles", "params":{"changes":[{"uri":tree.uri("syl.toml"),"type":2}]}})).await;
    client.settle().await;
    tokio::time::advance(std::time::Duration::from_millis(150)).await;
    let publications = [client.recv().await, client.recv().await];
    assert!(
        publications
            .iter()
            .any(|p| p["params"]["uri"] == tree.uri("main.syl").as_str())
    );
    let manifest = publications
        .iter()
        .find(|p| p["params"]["uri"] == tree.uri("syl.toml").as_str())
        .unwrap();
    assert!(
        manifest["params"]["diagnostics"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
