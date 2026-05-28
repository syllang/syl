use std::{
    fs,
    path::{Path, PathBuf},
};

#[test]
fn workspace_crates_have_publish_metadata() {
    let workspace = workspace_root();
    let mut violations = Vec::new();

    for manifest_path in crate_manifests(&workspace) {
        let manifest = read_text(manifest_path.clone());
        let package_name = package_name(&manifest).unwrap_or_else(|| {
            panic!(
                "crate manifest must declare a package name: {}",
                manifest_path.display()
            )
        });

        if package_field(&manifest, "description").is_none() {
            violations.push(format!("{package_name}: missing package.description"));
        }
        if package_field(&manifest, "license").is_none()
            && !package_field_uses_workspace(&manifest, "license")
        {
            violations.push(format!("{package_name}: missing package.license"));
        }
        if package_field(&manifest, "documentation").is_none()
            && package_field(&manifest, "homepage").is_none()
            && package_field(&manifest, "repository").is_none()
        {
            violations.push(format!(
                "{package_name}: missing package documentation, homepage, or repository URL"
            ));
        }

        let Some(readme) = package_field(&manifest, "readme") else {
            violations.push(format!("{package_name}: missing package.readme"));
            continue;
        };
        let readme_path = manifest_path
            .parent()
            .expect("crate manifest should have a parent directory")
            .join(readme);
        if !readme_path.is_file() {
            violations.push(format!(
                "{package_name}: package.readme does not exist at {}",
                readme_path.display()
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "workspace crates must keep publish metadata complete.\n{}",
        violations.join("\n")
    );
}

fn crate_manifests(workspace: &Path) -> Vec<PathBuf> {
    let crates_root = workspace.join("crates");
    let mut manifests = Vec::new();
    for entry in fs::read_dir(&crates_root)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", crates_root.display()))
    {
        let entry = entry.unwrap_or_else(|error| {
            panic!("failed to read entry in {}: {error}", crates_root.display())
        });
        let manifest = entry.path().join("Cargo.toml");
        if manifest.is_file() {
            manifests.push(manifest);
        }
    }
    manifests.sort();
    manifests
}

fn package_name(manifest: &str) -> Option<String> {
    package_field(manifest, "name")
}

fn package_field(manifest: &str, field: &str) -> Option<String> {
    package_section(manifest).lines().find_map(|line| {
        let line = line.split_once('#').map(|(head, _)| head).unwrap_or(line);
        let (key, value) = line.split_once('=')?;
        if key.trim() != field {
            return None;
        }
        let value = value.trim();
        Some(value.trim_matches('"').to_string())
    })
}

fn package_field_uses_workspace(manifest: &str, field: &str) -> bool {
    package_section(manifest).lines().any(|line| {
        let line = line.split_once('#').map(|(head, _)| head).unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            return false;
        };
        key.trim() == format!("{field}.workspace") && value.trim() == "true"
    })
}

fn package_section(manifest: &str) -> &str {
    let header = "[package]";
    let start = manifest
        .find(header)
        .unwrap_or_else(|| panic!("missing [package] section"));
    let tail = &manifest[start + header.len()..];
    let end = tail.find("\n[").unwrap_or(tail.len());
    &tail[..end]
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
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}
