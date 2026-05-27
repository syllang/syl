use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[test]
fn architecture_readmes_cover_contract_sections() {
    for contract in crate_contracts() {
        let readme_path = workspace_root()
            .join("crates")
            .join(contract.name)
            .join("README.md");
        let readme = read_text(&readme_path);

        for heading in required_readme_headings() {
            assert!(
                readme.contains(heading),
                "{} README is missing required heading {:?}",
                contract.name,
                heading
            );
        }

        for mention in contract.readme_mentions {
            assert!(
                readme.contains(mention),
                "{} README must mention {:?} to keep the contract specific",
                contract.name,
                mention
            );
        }
    }
}

#[test]
fn architecture_manifests_match_dependency_contract() {
    for contract in crate_contracts() {
        let manifest_path = workspace_root()
            .join("crates")
            .join(contract.name)
            .join("Cargo.toml");
        let manifest = read_text(&manifest_path);

        let dependencies = workspace_dependency_names(dependency_names(manifest_section(
            &manifest,
            "dependencies",
        )));
        let dev_dependencies = workspace_dependency_names(optional_dependency_names(
            optional_manifest_section(&manifest, "dev-dependencies"),
        ));

        assert_eq!(
            dependencies,
            set_of(contract.dependencies),
            "{} [dependencies] drifted from the crate dependency contract",
            contract.name
        );
        assert_eq!(
            dev_dependencies,
            set_of(contract.dev_dependencies),
            "{} [dev-dependencies] drifted from the crate dependency contract",
            contract.name
        );
    }
}

#[test]
fn architecture_workspace_dependencies_stay_one_way_and_acyclic() {
    let contracts = crate_contracts();
    let mut ranks = BTreeMap::new();
    let mut graph = BTreeMap::new();

    for contract in contracts {
        ranks.insert(contract.name, contract.rank);
        graph.insert(
            contract.name,
            manifest_workspace_dependencies(contract.name),
        );
    }

    for (crate_name, dependencies) in &graph {
        let rank = *ranks
            .get(crate_name)
            .unwrap_or_else(|| panic!("missing rank for {crate_name}"));
        for dependency in dependencies {
            let dependency_rank = *ranks.get(dependency.as_str()).unwrap_or_else(|| {
                panic!("missing rank for workspace dependency {crate_name} -> {dependency}")
            });
            assert!(
                rank > dependency_rank,
                "crate dependency direction violation: {crate_name} (rank {rank}) depends on {dependency} (rank {dependency_rank})"
            );
        }
    }

    let cycles = dependency_cycles(&graph);
    assert!(
        cycles.is_empty(),
        "workspace dependency graph must stay acyclic.\n{}",
        cycles.join("\n")
    );
}

#[test]
fn architecture_elab_public_surface_stays_out_of_frontend_semantic_api() {
    let elab_lib = read_text(&workspace_root().join("crates/syl_elab/src/lib.rs"));
    for forbidden in [
        "MiddleCompiler",
        "MiddleSession",
        "HirStage",
        "HirStageOutput",
        "TirStage",
        "TirStageOutput",
        "DefinitionInfo",
        "HoverInfo",
    ] {
        assert!(
            !elab_lib.contains(forbidden),
            "syl_elab public surface must not re-export {forbidden}"
        );
    }

    let elab_source = read_text(&workspace_root().join("crates/syl_elab/src/pipeline.rs"));
    for forbidden in [
        "pub struct MiddleSession",
        "pub struct HirStage",
        "pub struct TirStage",
        "pub struct DefinitionInfo",
        "pub struct HoverInfo",
        "pub fn resolve_hir",
        "pub fn check_tir",
        "pub fn completion_items_at",
        "pub fn hover_at",
        "pub fn definition_at",
    ] {
        assert!(
            !elab_source.contains(forbidden),
            "syl_elab pipeline must not publish frontend semantic API: {forbidden}"
        );
    }
    assert!(
        elab_source.contains("pub struct HardwareCompiler"),
        "syl_elab must expose a hardware compiler boundary instead of frontend stages"
    );
}

#[test]
fn architecture_session_and_query_use_sema_accessors_not_elab_stage_api() {
    let session_model =
        read_text(&workspace_root().join("crates/syl_session/src/snapshot/model.rs"));
    let session_cache =
        read_text(&workspace_root().join("crates/syl_session/src/snapshot/semantic_cache.rs"));
    let query_api = read_text(&workspace_root().join("crates/syl_query/src/snapshot/api.rs"));

    for forbidden in ["pub fn hir_stage", "pub fn tir_stage"] {
        assert!(
            !session_model.contains(forbidden),
            "syl_session snapshot API must not expose removed elab stage accessors: {forbidden}"
        );
    }
    assert!(
        session_model.contains("pub fn hir_analysis"),
        "syl_session snapshot should forward sema-owned HIR analysis access"
    );
    assert!(
        session_model.contains("pub fn tir_analysis"),
        "syl_session snapshot should forward sema-owned TIR analysis access"
    );

    for forbidden in ["HirStage", "TirStage", "MiddleCompiler", "TirStageOutput"] {
        assert!(
            !session_cache.contains(forbidden),
            "syl_session semantic cache must not depend on elab frontend stage types: {forbidden}"
        );
    }
    for required in [
        "SemanticCompiler",
        "HirAnalysis",
        "TirAnalysis",
        "HardwareCompiler",
        "ElaborationOutput",
    ] {
        assert!(
            session_cache.contains(required),
            "syl_session semantic cache should use the new sema/elab boundary: {required}"
        );
    }

    for forbidden in ["hir_stage()", "tir_stage()", "syl_elab::", "use syl_elab"] {
        assert!(
            !query_api.contains(forbidden),
            "syl_query must not depend on elab frontend stage API: {forbidden}"
        );
    }
    for required in [
        "hir_analysis_for_uri_with_token(",
        "tir_analysis_for_uri_with_token(",
    ] {
        assert!(
            query_api.contains(required),
            "syl_query should read sema analysis through session accessors: {required}"
        );
    }
}

struct CrateContract {
    name: &'static str,
    rank: usize,
    dependencies: &'static [&'static str],
    dev_dependencies: &'static [&'static str],
    readme_mentions: &'static [&'static str],
}

fn crate_contracts() -> &'static [CrateContract] {
    &[
        CrateContract {
            name: "syl",
            rank: 9,
            dependencies: &[
                "syl_emit",
                "syl_query",
                "syl_session",
                "syl_span",
                "syl_syntax",
            ],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_session",
                "syl_query",
                "syl_emit",
                "syl_hir",
                "syl_sema",
                "syl_elab",
                "Public Surface Policy",
            ],
        },
        CrateContract {
            name: "syl_span",
            rank: 0,
            dependencies: &[],
            dev_dependencies: &[],
            readme_mentions: &["`std` only", "syl_syntax", "syl_sema", "syl_elab", "syl_hw"],
        },
        CrateContract {
            name: "syl_syntax",
            rank: 1,
            dependencies: &["syl_span"],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_span",
                "syl_hir",
                "syl_sema",
                "syl_elab",
                "error recovery",
            ],
        },
        CrateContract {
            name: "syl_hir",
            rank: 2,
            dependencies: &["syl_span", "syl_syntax"],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_span",
                "syl_syntax",
                "syl_sema",
                "syl_elab",
                "stable IDs",
            ],
        },
        CrateContract {
            name: "syl_sema",
            rank: 3,
            dependencies: &["syl_hir", "syl_span", "syl_syntax"],
            dev_dependencies: &["syl_elab", "syl_emit", "syl_hw"],
            readme_mentions: &[
                "syl_hir",
                "syl_syntax",
                "syl_span",
                "syl_elab",
                "syl_emit",
                "syl_hw",
                "semantic side tables",
                "semantic hover/definition/completion",
            ],
        },
        CrateContract {
            name: "syl_elab",
            rank: 4,
            dependencies: &["syl_hir", "syl_hw", "syl_sema", "syl_span"],
            dev_dependencies: &["syl_syntax"],
            readme_mentions: &[
                "syl_hir",
                "syl_sema",
                "syl_hw",
                "syl_emit",
                "TirAnalysis",
                "HardwareCompiler",
                "ParametricHwDesign",
            ],
        },
        CrateContract {
            name: "syl_hw",
            rank: 3,
            dependencies: &["syl_span"],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_span",
                "syl_elab",
                "syl_emit",
                "backend-neutral",
                "data contract",
            ],
        },
        CrateContract {
            name: "syl_emit",
            rank: 5,
            dependencies: &["syl_hw"],
            dev_dependencies: &[],
            readme_mentions: &["syl_hw", "syl_elab", "SystemVerilog", "backend entry point"],
        },
        CrateContract {
            name: "syl_session",
            rank: 6,
            dependencies: &["syl_elab", "syl_hw", "syl_sema", "syl_span", "syl_syntax"],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_elab",
                "syl_hw",
                "syl_sema",
                "syl_query",
                "AnalysisSnapshot",
                "semantic analysis",
                "workspace",
            ],
        },
        CrateContract {
            name: "syl_query",
            rank: 7,
            dependencies: &[
                "syl_hir",
                "syl_sema",
                "syl_session",
                "syl_span",
                "syl_syntax",
            ],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_hir",
                "syl_session",
                "syl_sema",
                "syl_syntax",
                "syl_elab",
                "syl_lsp",
                "protocol-neutral",
            ],
        },
        CrateContract {
            name: "syl_lsp",
            rank: 8,
            dependencies: &["syl_query", "syl_session", "syl_span"],
            dev_dependencies: &[],
            readme_mentions: &[
                "syl_query",
                "syl_session",
                "tower-lsp",
                "UTF-16",
                "syl_elab",
                "syl_emit",
            ],
        },
        CrateContract {
            name: "sylc",
            rank: 9,
            dependencies: &["syl_emit", "syl_session", "syl_span", "syl_syntax"],
            dev_dependencies: &["syl_elab", "syl_hw", "syl_query", "syl_sema"],
            readme_mentions: &[
                "syl_session",
                "syl_emit",
                "syl_elab",
                "syl_hir",
                "syl_sema",
                "CLI",
            ],
        },
    ]
}

fn required_readme_headings() -> [&'static str; 8] {
    [
        "## Responsibilities",
        "## Inputs",
        "## Outputs",
        "## Allowed Dependencies",
        "## Forbidden Dependencies",
        "## Allowed Responsibilities",
        "## Forbidden Responsibilities",
        "## Public Surface Policy",
    ]
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|path| path.parent())
        .expect("sylc crate should be nested under workspace/crates")
        .to_path_buf()
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn manifest_workspace_dependencies(crate_name: &str) -> Vec<String> {
    let manifest_path = workspace_root()
        .join("crates")
        .join(crate_name)
        .join("Cargo.toml");
    let manifest = read_text(&manifest_path);
    dependency_names(manifest_section(&manifest, "dependencies"))
        .into_iter()
        .filter(|dependency| {
            dependency == "syl" || dependency == "sylc" || dependency.starts_with("syl_")
        })
        .collect()
}

fn set_of(values: &[&str]) -> BTreeSet<String> {
    values.iter().map(|value| (*value).to_string()).collect()
}

fn dependency_names(section: &str) -> BTreeSet<String> {
    optional_dependency_names(Some(section))
}

fn workspace_dependency_names(dependencies: BTreeSet<String>) -> BTreeSet<String> {
    dependencies
        .into_iter()
        .filter(|name| name == "syl" || name == "sylc" || name.starts_with("syl_"))
        .collect()
}

fn optional_dependency_names(section: Option<&str>) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();
    let Some(section) = section else {
        return dependencies;
    };

    for line in section.lines() {
        let line = line
            .split_once('#')
            .map(|(head, _)| head)
            .unwrap_or(line)
            .trim();
        if line.is_empty() {
            continue;
        }
        let Some((key, _)) = line.split_once('=') else {
            continue;
        };
        let Some(name) = key.trim().split('.').next() else {
            continue;
        };
        if !name.is_empty() {
            dependencies.insert(name.to_string());
        }
    }

    dependencies
}

fn manifest_section<'a>(manifest: &'a str, section: &str) -> &'a str {
    optional_manifest_section(manifest, section)
        .unwrap_or_else(|| panic!("missing [{section}] section in manifest"))
}

fn optional_manifest_section<'a>(manifest: &'a str, section: &str) -> Option<&'a str> {
    let header = format!("[{section}]");
    let start = manifest.find(&header)?;
    let tail = &manifest[start + header.len()..];
    let end = tail.find("\n[").unwrap_or(tail.len());
    Some(&tail[..end])
}

fn dependency_cycles(graph: &BTreeMap<&'static str, Vec<String>>) -> Vec<String> {
    let mut state = BTreeMap::new();
    let mut stack = Vec::new();
    let mut cycles = BTreeSet::new();

    for crate_name in graph.keys().copied() {
        visit_for_cycles(crate_name, graph, &mut state, &mut stack, &mut cycles);
    }

    cycles.into_iter().collect()
}

fn visit_for_cycles(
    crate_name: &'static str,
    graph: &BTreeMap<&'static str, Vec<String>>,
    state: &mut BTreeMap<&'static str, VisitState>,
    stack: &mut Vec<&'static str>,
    cycles: &mut BTreeSet<String>,
) {
    match state.get(crate_name) {
        Some(VisitState::Visiting) => {
            if let Some(start) = stack.iter().position(|entry| *entry == crate_name) {
                let mut cycle: Vec<_> = stack[start..]
                    .iter()
                    .map(|entry| (*entry).to_string())
                    .collect();
                cycle.push(crate_name.to_string());
                cycles.insert(cycle.join(" -> "));
            }
            return;
        }
        Some(VisitState::Visited) => return,
        None => {}
    }

    state.insert(crate_name, VisitState::Visiting);
    stack.push(crate_name);

    if let Some(dependencies) = graph.get(crate_name) {
        for dependency in dependencies {
            let Some(next) = graph
                .keys()
                .copied()
                .find(|candidate| *candidate == dependency)
            else {
                continue;
            };
            visit_for_cycles(next, graph, state, stack, cycles);
        }
    }

    stack.pop();
    state.insert(crate_name, VisitState::Visited);
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Visited,
}
