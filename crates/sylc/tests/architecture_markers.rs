use std::{
    fs,
    path::{Component, Path, PathBuf},
};

#[test]
fn architecture_debt_markers_match_quality_gate_script() {
    let workspace = workspace_root();
    let self_path = workspace.join("crates/sylc/tests/architecture_markers.rs");
    let violations = unresolved_debt_markers(&workspace, &self_path);

    assert!(
        violations.is_empty(),
        "unresolved debt markers must not remain anywhere in the workspace.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_manifests_stay_on_facade_edges() {
    let workspace = workspace_root();

    let syl_manifest = read_text(workspace.join("crates/syl/Cargo.toml"));
    let syl_dependencies = manifest_section(&syl_manifest, "dependencies");
    assert_dependency_section_contains_all(
        "syl",
        &syl_dependencies,
        &[
            "syl_emit",
            "syl_query",
            "syl_session",
            "syl_span",
            "syl_syntax",
        ],
    );
    assert_dependency_section_lacks(
        "syl",
        &syl_dependencies,
        &["syl_elab", "syl_sema", "syl_hw", "syl_lsp", "tower-lsp"],
    );

    let lsp_manifest = read_text(workspace.join("crates/syl_lsp/Cargo.toml"));
    assert_manifest_contains_all(
        "syl_lsp",
        &lsp_manifest,
        &["syl_query", "syl_session", "syl_span", "tokio", "tower-lsp"],
    );
    assert_manifest_lacks(
        "syl_lsp",
        &lsp_manifest,
        &["syl_hir", "syl_sema", "syl_elab", "syl_hw", "syl_emit"],
    );

    let session_manifest = read_text(workspace.join("crates/syl_session/Cargo.toml"));
    assert_manifest_contains_all(
        "syl_session",
        &session_manifest,
        &["syl_elab", "syl_hw", "syl_sema", "syl_span", "syl_syntax"],
    );
    assert_manifest_lacks("syl_session", &session_manifest, &["syl_query"]);

    let query_manifest = read_text(workspace.join("crates/syl_query/Cargo.toml"));
    assert_manifest_contains_all("syl_query", &query_manifest, &["syl_session"]);
    assert_manifest_lacks(
        "syl_query",
        &query_manifest,
        &["syl_elab", "syl_hw", "tower-lsp", "lsp_types", "url"],
    );

    let emit_manifest = read_text(workspace.join("crates/syl_emit/Cargo.toml"));
    assert_manifest_contains_all("syl_emit", &emit_manifest, &["syl_hw", "thiserror"]);
    assert_manifest_lacks(
        "syl_emit",
        &emit_manifest,
        &["syl_hir", "syl_sema", "syl_elab"],
    );

    let hw_manifest = read_text(workspace.join("crates/syl_hw/Cargo.toml"));
    assert_manifest_contains_all("syl_hw", &hw_manifest, &["syl_span"]);
    assert_manifest_lacks(
        "syl_hw",
        &hw_manifest,
        &["syl_hir", "syl_sema", "syl_elab", "syl_emit"],
    );

    let sylc_manifest = read_text(workspace.join("crates/sylc/Cargo.toml"));
    let dependencies = manifest_section(&sylc_manifest, "dependencies");
    let dev_dependencies = manifest_section(&sylc_manifest, "dev-dependencies");
    assert!(
        !dependencies.contains("syl_elab"),
        "sylc normal dependencies must not depend on syl_elab\n{}",
        dependencies
    );
    assert!(
        dev_dependencies.contains("syl_elab"),
        "sylc white-box tests must keep syl_elab only as a dev-dependency\n{}",
        dev_dependencies
    );
}

#[test]
fn architecture_active_sources_do_not_reference_old_project_name() {
    let workspace = workspace_root();
    let roots = [
        workspace.join("crates/syl_query/src"),
        workspace.join("crates/syl_session/src"),
        workspace.join("crates/syl_lsp/src"),
        workspace.join("crates/sylc/src"),
    ];
    let forbidden = [["syl", "_project"].concat(), ["syl", "_middle"].concat()];
    let mut violations = Vec::new();

    for root in roots {
        for path in rs_files_under(&root) {
            let text = read_text(path.clone());
            let active_source = text
                .rsplit_once("#[cfg(test)]")
                .map(|(head, _)| head)
                .unwrap_or(&text);
            for pattern in &forbidden {
                if active_source.contains(pattern) {
                    violations.push(format!(
                        "{} contains forbidden active-source pattern {:?}",
                        path.strip_prefix(&workspace)
                            .unwrap_or(path.as_path())
                            .display(),
                        pattern
                    ));
                }
            }
        }
    }

    assert!(
        violations.is_empty(),
        "old project-era source strings must not remain in active code.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_facades_do_not_import_compiler_internals() {
    let workspace = workspace_root();
    let syl_root = workspace.join("crates/syl/src");
    let lsp_root = workspace.join("crates/syl_lsp/src");
    let sylc_root = workspace.join("crates/sylc/src");
    let mut violations = Vec::new();

    violations.extend(source_pattern_violations(
        &workspace,
        &syl_root,
        &["syl_elab", "syl_sema", "syl_hw", "syl_lsp", "tower_lsp"],
    ));
    violations.extend(source_pattern_violations(
        &workspace,
        &lsp_root,
        &["syl_hir", "syl_sema", "syl_elab", "syl_hw"],
    ));
    violations.extend(source_pattern_violations(
        &workspace,
        &sylc_root,
        &["syl_elab"],
    ));

    assert!(
        violations.is_empty(),
        "facade crates must not import compiler internals directly.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_rust_files_stay_under_700_lines() {
    let workspace = workspace_root();
    let mut violations = Vec::new();

    for path in rs_files_under(&workspace) {
        let line_count = read_text(path.clone()).lines().count();
        if line_count > 700 {
            violations.push(format!(
                "{} has {} lines",
                path.strip_prefix(&workspace)
                    .unwrap_or(path.as_path())
                    .display(),
                line_count
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "Rust source files must remain under 700 lines.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_module_paths_do_not_repeat_crate_domain() {
    let workspace = workspace_root();
    let violations = repeated_crate_domain_paths(&workspace);

    assert!(
        violations.is_empty(),
        "source module paths must not repeat their crate domain prefix.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_must_not_reintroduce_span_based_semantic_identity_fallback() {
    let workspace = workspace_root();
    let source_root = workspace.join("crates/syl_elab/src");
    let mut violations = Vec::new();

    for path in rs_files_under(&source_root) {
        let text = read_text(path.clone());
        let normalized = normalize_whitespace(&text);

        for pattern in forbidden_elab_span_fallback_patterns() {
            if normalized.contains(pattern.snippet) {
                violations.push(format!(
                    "{} contains forbidden pattern {:?} ({})",
                    path.strip_prefix(&workspace)
                        .unwrap_or(path.as_path())
                        .display(),
                    pattern.snippet,
                    pattern.reason
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "span-based semantic identity fallback must stay out of syl_elab.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_must_not_reintroduce_hir_span_key_compatibility() {
    let workspace = workspace_root();
    let source_root = workspace.join("crates/syl_hir/src");
    let mut violations = Vec::new();

    for path in rs_files_under(&source_root) {
        let text = read_text(path.clone());
        let normalized = normalize_whitespace(&text);

        for pattern in forbidden_hir_compatibility_patterns() {
            if normalized.contains(pattern.snippet) {
                violations.push(format!(
                    "{} contains forbidden pattern {:?} ({})",
                    path.strip_prefix(&workspace)
                        .unwrap_or(path.as_path())
                        .display(),
                    pattern.snippet,
                    pattern.reason
                ));
            }
        }
    }

    assert!(
        violations.is_empty(),
        "span-key compatibility fallbacks must stay out of syl_hir.\n{}",
        violations.join("\n")
    );
}

#[test]
fn architecture_white_box_inventory_matches_current_internal_imports() {
    let workspace = workspace_root();
    let mut actual: Vec<_> = remaining_sylc_internal_test_imports(&workspace)
        .into_iter()
        .map(|path| {
            path.strip_prefix(&workspace)
                .unwrap_or(path.as_path())
                .display()
                .to_string()
        })
        .collect();
    actual.sort();

    assert_eq!(
        actual,
        internal_test_inventory(),
        "sylc tests that import compiler internals must stay explicitly inventoried"
    );
}

fn repeated_crate_domain_paths(workspace: &Path) -> Vec<String> {
    let crates_root = workspace.join("crates");
    let mut violations = Vec::new();
    let Ok(entries) = fs::read_dir(&crates_root) else {
        return violations;
    };

    for entry in entries.filter_map(Result::ok) {
        let crate_path = entry.path();
        if !crate_path.is_dir() {
            continue;
        }
        let Some(crate_name) = crate_path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let domain = crate_name.strip_prefix("syl_").unwrap_or(crate_name);
        let src_root = crate_path.join("src");
        if !src_root.exists() {
            continue;
        }
        collect_repeated_domain_paths(workspace, &src_root, domain, &mut violations);
    }

    violations.sort();
    violations
}

fn internal_test_inventory() -> Vec<String> {
    [
        "crates/sylc/tests/architecture_backend_emit.rs",
        "crates/sylc/tests/architecture_elaboration.rs",
        "crates/sylc/tests/architecture_opaque_summaries.rs",
        "crates/sylc/tests/architecture_query_lsp.rs",
        "crates/sylc/tests/architecture_semantic_analysis.rs",
        "crates/sylc/tests/architecture_std_sources.rs",
        "crates/sylc/tests/conformance.rs",
        "crates/sylc/tests/driver_overlap.rs",
        "crates/sylc/tests/interface_regression.rs",
        "crates/sylc/tests/support/mod.rs",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn collect_repeated_domain_paths(
    workspace: &Path,
    src_root: &Path,
    domain: &str,
    violations: &mut Vec<String>,
) {
    for path in rs_files_under(src_root) {
        let Ok(relative) = path.strip_prefix(src_root) else {
            continue;
        };
        let Some(component) = repeated_domain_component(relative, domain) else {
            continue;
        };
        violations.push(format!(
            "{} repeats crate domain prefix {:?} in {:?}",
            path.strip_prefix(workspace)
                .unwrap_or(path.as_path())
                .display(),
            domain,
            component
        ));
    }
}

fn repeated_domain_component(path: &Path, domain: &str) -> Option<String> {
    for component in path.components() {
        let Component::Normal(raw) = component else {
            continue;
        };
        let name = raw.to_str()?;
        let stem = name.strip_suffix(".rs").unwrap_or(name);
        if matches!(stem, "lib" | "main") {
            continue;
        }
        if stem == domain
            || stem
                .strip_prefix(domain)
                .is_some_and(|rest| rest.starts_with('_') || rest.starts_with('-'))
        {
            return Some(stem.to_string());
        }
    }
    None
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|path| path.parent())
        .expect("sylc crate should be nested under workspace/crates")
        .to_path_buf()
}

fn unresolved_debt_markers(workspace: &Path, self_path: &Path) -> Vec<String> {
    let mut violations = Vec::new();
    let mut files = Vec::new();
    collect_files(workspace, &mut files);
    files.sort();

    for path in files {
        if path == self_path {
            continue;
        }
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(_) => continue,
        };

        for (line_index, line) in text.lines().enumerate() {
            for marker in debt_markers() {
                if let Some(column_index) = line.find(marker) {
                    violations.push(format!(
                        "{}:{}:{} contains unresolved debt marker {marker}",
                        path.strip_prefix(workspace)
                            .unwrap_or(path.as_path())
                            .display(),
                        line_index + 1,
                        column_index + 1
                    ));
                }
            }
        }
    }

    violations
}

fn debt_markers() -> [&'static str; 6] {
    ["TODO", "FIXME", "MUST_FIX", "SHOULD_FIX", "HACK", "XXX"]
}

fn source_pattern_violations(workspace: &Path, root: &Path, patterns: &[&str]) -> Vec<String> {
    let mut violations = Vec::new();

    for path in rs_files_under(root) {
        let text = read_text(path.clone());
        let active_source = text
            .rsplit_once("#[cfg(test)]")
            .map(|(head, _)| head)
            .unwrap_or(&text);
        for (line_index, line) in active_source.lines().enumerate() {
            let trimmed = line.trim_start();
            if !trimmed.starts_with("use ") && !trimmed.starts_with("pub use ") {
                continue;
            }
            for pattern in patterns {
                if trimmed.contains(pattern) {
                    violations.push(format!(
                        "{}:{} contains forbidden import pattern {:?}",
                        path.strip_prefix(workspace)
                            .unwrap_or(path.as_path())
                            .display(),
                        line_index + 1,
                        pattern
                    ));
                }
            }
        }
    }

    violations
}

fn assert_manifest_contains_all(name: &str, manifest: &str, required: &[&str]) {
    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|pattern| !manifest.contains(pattern))
        .collect();
    assert!(
        missing.is_empty(),
        "{} manifest is missing required dependency markers: {:?}\n{}",
        name,
        missing,
        manifest
    );
}

fn assert_manifest_lacks(name: &str, manifest: &str, forbidden: &[&str]) {
    let present: Vec<_> = forbidden
        .iter()
        .copied()
        .filter(|pattern| manifest.contains(pattern))
        .collect();
    assert!(
        present.is_empty(),
        "{} manifest still contains forbidden dependency markers: {:?}\n{}",
        name,
        present,
        manifest
    );
}

fn assert_dependency_section_contains_all(name: &str, section: &str, required: &[&str]) {
    let missing: Vec<_> = required
        .iter()
        .copied()
        .filter(|dependency| !dependency_section_declares(section, dependency))
        .collect();
    assert!(
        missing.is_empty(),
        "{} dependency section is missing required dependencies: {:?}\n{}",
        name,
        missing,
        section
    );
}

fn assert_dependency_section_lacks(name: &str, section: &str, forbidden: &[&str]) {
    let present: Vec<_> = forbidden
        .iter()
        .copied()
        .filter(|dependency| dependency_section_declares(section, dependency))
        .collect();
    assert!(
        present.is_empty(),
        "{} dependency section declares forbidden dependencies: {:?}\n{}",
        name,
        present,
        section
    );
}

fn dependency_section_declares(section: &str, dependency: &str) -> bool {
    section.lines().any(|line| {
        let line = line.split_once('#').map(|(head, _)| head).unwrap_or(line);
        let Some((key, _)) = line.split_once('=') else {
            return false;
        };
        key.trim().split('.').next() == Some(dependency)
    })
}

fn manifest_section(manifest: &str, section: &str) -> String {
    let header = format!("[{section}]");
    let start = manifest
        .find(&header)
        .unwrap_or_else(|| panic!("missing [{section}] section in manifest"));
    let tail = &manifest[start + header.len()..];
    let end = tail.find("\n[").unwrap_or(tail.len());
    tail[..end].to_string()
}

fn read_text(path: PathBuf) -> String {
    fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn remaining_sylc_internal_test_imports(workspace: &Path) -> Vec<PathBuf> {
    let test_root = workspace.join("crates/sylc/tests");
    let mut paths = Vec::new();
    for path in rs_files_under(&test_root) {
        if path.file_name().and_then(|name| name.to_str()) == Some("architecture_markers.rs") {
            continue;
        }
        let text = read_text(path.clone());
        if text.lines().any(imports_compiler_internal) {
            paths.push(path);
        }
    }
    paths.sort();
    paths
}

fn imports_compiler_internal(line: &str) -> bool {
    let trimmed = line.trim_start();
    (trimmed.starts_with("use ") || trimmed.starts_with("pub use "))
        && ["syl_elab", "syl_hw", "syl_sema", "syl_hir"]
            .iter()
            .any(|pattern| trimmed.contains(pattern))
}

fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => panic!("failed to read directory {}: {error}", dir.display()),
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => panic!("failed to read entry in {}: {error}", dir.display()),
        };
        let path = entry.path();
        if should_skip_path(&path) {
            continue;
        }
        if path.is_dir() {
            collect_files(&path, files);
        } else {
            files.push(path);
        }
    }
}

struct ForbiddenHirCompatibilityPattern {
    snippet: &'static str,
    reason: &'static str,
}

struct ForbiddenElabSpanFallbackPattern {
    snippet: &'static str,
    reason: &'static str,
}

fn forbidden_elab_span_fallback_patterns() -> [ForbiddenElabSpanFallbackPattern; 5] {
    [
        ForbiddenElabSpanFallbackPattern {
            snippet: "resolved_type_def_for_span(",
            reason: "fallback type-definition lookup by source span",
        },
        ForbiddenElabSpanFallbackPattern {
            snippet: "struct ElabTypeRef",
            reason: "old span-indexed type reference cache",
        },
        ForbiddenElabSpanFallbackPattern {
            snippet: "type_refs: Vec<ElabTypeRef>",
            reason: "old span-indexed type reference table",
        },
        ForbiddenElabSpanFallbackPattern {
            snippet: "filter(|type_ref| type_ref.contains(span))",
            reason: "span containment used as semantic identity",
        },
        ForbiddenElabSpanFallbackPattern {
            snippet: "min_by_key(|type_ref| type_ref.width())",
            reason: "narrowest source-span fallback selection",
        },
    ]
}

fn forbidden_hir_compatibility_patterns() -> [ForbiddenHirCompatibilityPattern; 3] {
    [
        ForbiddenHirCompatibilityPattern {
            snippet: "Legacy source-span projections retained only for compatibility fallbacks.",
            reason: "span-key compatibility comment",
        },
        ForbiddenHirCompatibilityPattern {
            snippet: "mod keys;",
            reason: "span-key compatibility module",
        },
        ForbiddenHirCompatibilityPattern {
            snippet: "pub use keys::{HirExprKey, HirLocalKey};",
            reason: "span-key compatibility reexport",
        },
    ]
}

fn rs_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_rs_files(root, &mut files);
    files
}

fn collect_rs_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()));

    for entry in entries {
        let entry = entry
            .unwrap_or_else(|error| panic!("failed to read entry in {}: {error}", dir.display()));
        let path = entry.path();
        if should_skip_path(&path) {
            continue;
        }
        if path.is_dir() {
            collect_rs_files(&path, files);
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("rs") {
            files.push(path);
        }
    }
}

fn should_skip_path(path: &Path) -> bool {
    path.components().any(|component| {
        matches!(
            component.as_os_str().to_str(),
            Some(".git" | ".tmp" | "target")
        )
    })
}

fn normalize_whitespace(text: &str) -> String {
    let mut normalized = String::with_capacity(text.len());
    let mut in_whitespace = false;

    for ch in text.chars() {
        if ch.is_whitespace() {
            if !in_whitespace {
                normalized.push(' ');
                in_whitespace = true;
            }
        } else {
            normalized.push(ch);
            in_whitespace = false;
        }
    }

    normalized
}
