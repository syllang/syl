use super::*;
use crate::AnalysisHost;
use std::{
    collections::BTreeMap,
    fs,
    sync::atomic::{AtomicU64, Ordering},
};
use syl_sema::{OpaqueItemKind, OpaqueItemSummary};

#[derive(Debug, Default)]
pub(super) struct TestVfs {
    pub files: BTreeMap<PathBuf, String>,
}
impl TestVfs {
    pub fn insert(&mut self, path: &str, text: &str) {
        self.files.insert(path.into(), text.into());
    }
}
impl Vfs for TestVfs {
    fn read_to_string(&self, path: &Path) -> Result<String, ProjectError> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| ProjectError::Read {
                path: path.into(),
                source: std::io::ErrorKind::NotFound.into(),
            })
    }
    fn exists(&self, path: &Path) -> bool {
        self.files.contains_key(path)
    }
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, ProjectError> {
        Ok(normalize_lexical(path))
    }
}

pub(super) struct TempDir(pub PathBuf);
impl TempDir {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "syl-manifest-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    pub fn write(&self, path: &str, text: &str) -> PathBuf {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
}
impl Drop for TempDir {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn empty_manifest_defaults_and_explicit_empty_roots() {
    let dir = TempDir::new();
    let path = dir.write("syl.toml", "");
    let manifest = ProjectManifest::load(&path).unwrap();
    assert_eq!(ProjectManifest::FILE_NAME, MANIFEST_FILE_NAME);
    assert_eq!(manifest.manifest_path(), path);
    assert_eq!(manifest.root_dir(), dir.0);
    assert_eq!(manifest.workspace_roots(), std::slice::from_ref(&dir.0));
    assert_eq!(
        (manifest.name(), manifest.version(), manifest.edition()),
        (None, None, None)
    );
    assert!(manifest.std_roots().is_empty());
    assert!(manifest.package_roots().is_empty());
    assert!(manifest.opaque_summary_paths().is_empty());
    dir.write("syl.toml", "[paths]\nworkspace_roots = []");
    assert!(
        ProjectManifest::load(&path)
            .unwrap()
            .workspace_roots()
            .is_empty()
    );
}

#[test]
fn configured_paths_resolve_and_deduplicate_in_declaration_order() {
    let dir = TempDir::new();
    fs::create_dir(dir.0.join("src")).unwrap();
    let path = dir.write(
        "syl.toml",
        &format!(
            r#"
[project]
name = "demo"
version = "0.1.0"
edition = "2026"
[paths]
workspace_roots = ["./src", "src/../src", "{}", "absent/../next"]
std_roots = ["../std", "../std/."]
package_roots = ["deps", "./deps", "other"]
opaque_summaries = ["vendor/s.json", "./vendor/s.json"]
"#,
            dir.0.display()
        ),
    );
    let manifest = ProjectManifest::load(&path).unwrap();
    assert_eq!(
        (manifest.name(), manifest.version(), manifest.edition()),
        (Some("demo"), Some("0.1.0"), Some("2026"))
    );
    assert_eq!(
        manifest.workspace_roots(),
        [dir.0.join("src"), dir.0.clone(), dir.0.join("next")]
    );
    assert_eq!(manifest.std_roots(), [dir.0.parent().unwrap().join("std")]);
    assert_eq!(
        manifest.package_roots(),
        [dir.0.join("deps"), dir.0.join("other")]
    );
    assert_eq!(
        manifest.opaque_summary_paths(),
        [dir.0.join("vendor/s.json")]
    );
    let config = manifest.to_project_config();
    assert_eq!(config.workspace_roots(), manifest.workspace_roots());
    assert_eq!(config.std_roots(), manifest.std_roots());
    assert_eq!(config.package_roots(), manifest.package_roots());
}

#[test]
fn discovery_uses_nearest_ancestor_for_files_directories_and_direct_manifest() {
    let dir = TempDir::new();
    let outer = dir.write("syl.toml", "");
    let inner = dir.write("nested/syl.toml", "");
    let source = dir.write("nested/src/main.syl", "");
    for start in [&source, source.parent().unwrap(), &inner] {
        assert_eq!(
            ProjectManifest::discover(start).unwrap(),
            Some(inner.clone())
        );
        assert_eq!(
            ProjectManifest::discover_and_load(start)
                .unwrap()
                .unwrap()
                .manifest_path(),
            inner
        );
    }
    assert_eq!(ProjectManifest::discover(&dir.0).unwrap(), Some(outer));
    let mut vfs = TestVfs::default();
    vfs.insert("/virtual/syl.toml", "");
    vfs.insert("/virtual/nested/syl.toml", "");
    vfs.insert("/virtual/nested/src/main.syl", "");
    for start in [
        "/virtual/nested/src/main.syl",
        "/virtual/nested/src",
        "/virtual/nested/syl.toml",
    ] {
        let manifest = ProjectManifest::discover_and_load_with_vfs(Path::new(start), &vfs)
            .unwrap()
            .unwrap();
        assert_eq!(
            manifest.manifest_path(),
            Path::new("/virtual/nested/syl.toml")
        );
    }
    assert!(
        ProjectManifest::discover_with_vfs(Path::new("/unrelated"), &vfs)
            .unwrap()
            .is_none()
    );
    assert!(
        ProjectManifest::discover_with_vfs(Path::new("/"), &TestVfs::default())
            .unwrap()
            .is_none()
    );
    let relative = Path::new("virtual-relative/syl.toml");
    vfs.files.insert(
        std::env::current_dir().unwrap().join(relative),
        String::new(),
    );
    assert!(
        ProjectManifest::load_with_vfs(relative, &vfs)
            .unwrap()
            .manifest_path()
            .is_absolute()
    );
}

#[cfg(unix)]
#[test]
fn existing_symlinks_use_real_ancestor_chain_and_canonical_roots() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new();
    let manifest_path = dir.write(
        "real/syl.toml",
        "[paths]\nworkspace_roots = ['src', '../alias/src']",
    );
    let source = dir.write("real/src/main.syl", "");
    symlink(dir.0.join("real"), dir.0.join("alias")).unwrap();
    assert_eq!(
        ProjectManifest::discover(&dir.0.join("alias/src/main.syl")).unwrap(),
        Some(manifest_path.clone())
    );
    let manifest = ProjectManifest::load(&manifest_path).unwrap();
    assert_eq!(
        manifest.workspace_roots(),
        [source.parent().unwrap().to_path_buf()]
    );
}

#[test]
fn manifest_parse_validation_and_io_errors_remain_distinct() {
    let dir = TempDir::new();
    for source in [
        "[",
        "extra = 1",
        "[oops]",
        "[project]\nextra = 1",
        "[paths]\nstd_root = []",
    ] {
        let path = dir.write("syl.toml", source);
        assert!(
            matches!(ProjectManifest::load(&path), Err(ProjectError::ManifestParse { path: p, .. }) if p == path)
        );
    }
    for source in [
        "[project]\nname = '  '",
        "[paths]\nworkspace_roots = ['']",
        "[paths]\nstd_roots = ['  ']",
        "[paths]\npackage_roots = ['']",
        "[paths]\nopaque_summaries = ['']",
    ] {
        let path = dir.write("syl.toml", source);
        assert!(
            matches!(ProjectManifest::load(&path), Err(ProjectError::ManifestInvalid { path: p, .. }) if p == path)
        );
    }
    assert!(matches!(
        ProjectManifest::load(&dir.0.join("missing.toml")),
        Err(ProjectError::Read { .. })
    ));
}

#[test]
fn host_loads_trusted_summary_into_drc_and_preserves_existing_overlays() {
    let dir = TempDir::new();
    let path = dir.write("syl.toml", "[paths]\nopaque_summaries = ['vendor.json']");
    dir.write(
        "vendor.json",
        r#"{"schema_version":1,"summaries":[{
        "kind":"precompiled_cell","callable":"VendorLatch","driven_fields":["y"],
        "latency_class":"sequential","trust_boundary":"trusted_precompiled"
    }]}"#,
    );
    let source = dir.write("top.syl", "extern cell VendorLatch(y: in Bit)\ncell Top(y: out Bit) {\n signal tmp: Bit := 0\n let vendor = place VendorLatch(y: tmp)\n y := tmp\n}\n");
    let manifest = ProjectManifest::load(&path).unwrap();
    let mut host = AnalysisHost::from_manifest(&manifest).unwrap();
    let snapshot = host.load(&[source]).unwrap();
    assert!(snapshot.diagnostics().is_empty());
    assert!(
        snapshot
            .semantic_diagnostics()
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
    );
    host.register_opaque_summary(
        OpaqueItemSummary::builder(OpaqueItemKind::ExternCell, "Other").build(),
    );
    let revision = host.database().revision();
    host.load_manifest_opaque_summaries(&manifest).unwrap();
    assert!(host.opaque_summaries().get("Other").is_some());
    assert_eq!(host.database().revision(), revision);
    dir.write("vendor.json", "bad json");
    let summaries = host.opaque_summaries().clone();
    assert!(host.load_manifest_opaque_summaries(&manifest).is_err());
    assert_eq!(host.opaque_summaries(), &summaries);
    assert_eq!(host.database().revision(), revision);
}
