use crate::{AnalysisHost, DocumentUri, DocumentVersion, ProjectConfig, ProjectManifest};
use std::{
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

struct Tree(PathBuf);
impl Tree {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "syl-reconfigure-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}
impl Drop for Tree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn manifest_reconfiguration_preserves_overlays_and_rolls_back_invalid_metadata() {
    let tree = Tree::new();
    let root = &tree.0;
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/main.syl"), "cell Disk() {}\n").unwrap();
    fs::write(
        root.join("syl.toml"),
        "[paths]\nworkspace_roots=['src']\nopaque_summaries=['meta.json']\n",
    )
    .unwrap();
    fs::write(root.join("meta.json"), r#"{"schema_version":1,"summaries":[{"kind":"precompiled_cell","callable":"Vendor","trust_boundary":"trusted_precompiled"}]}"#).unwrap();
    let manifest = ProjectManifest::load(&root.join("syl.toml")).unwrap();
    let uri = DocumentUri::from_file_path(&root.join("src/main.syl"));
    let mut host = AnalysisHost::new();
    host.open_document(
        uri.clone(),
        "cell Unsaved() {}\n".into(),
        DocumentVersion::new(9),
    );
    host.apply_manifest(&manifest).unwrap();
    host.refresh_inputs(manifest.workspace_roots()).unwrap();
    assert!(host.opaque_summaries().get("Vendor").is_some());
    let revision = host.database().revision();
    host.refresh_inputs(manifest.workspace_roots()).unwrap();
    assert_eq!(host.database().revision(), revision);
    fs::write(root.join("meta.json"), "invalid").unwrap();
    assert!(host.apply_manifest(&manifest).is_err());
    assert_eq!(host.database().revision(), revision);
    assert!(host.opaque_summaries().get("Vendor").is_some());
    let snapshot = host.snapshot().unwrap();
    assert_eq!(snapshot.file_by_uri(&uri).unwrap().module_path(), &["main"]);
    assert_eq!(host.overlay(&uri).unwrap().text(), "cell Unsaved() {}\n");
    assert_eq!(
        host.overlay(&uri).unwrap().version(),
        DocumentVersion::new(9)
    );
    fs::write(root.join("src/second.syl"), "cell Second() {}\n").unwrap();
    host.refresh_inputs(manifest.workspace_roots()).unwrap();
    assert_eq!(host.roots().len(), 2);
    fs::remove_file(root.join("src/second.syl")).unwrap();
    host.refresh_inputs(manifest.workspace_roots()).unwrap();
    assert_eq!(host.roots().len(), 1);
    host.reconfigure(ProjectConfig::new());
    assert!(host.opaque_summaries().get("Vendor").is_some());
    host.clear_opaque_summaries();
    assert!(host.opaque_summaries().get("Vendor").is_none());
    let revision = host.database().revision();
    host.clear_opaque_summaries();
    assert_eq!(host.database().revision(), revision);
    assert_eq!(
        host.overlay(&uri).unwrap().version(),
        DocumentVersion::new(9)
    );
}
