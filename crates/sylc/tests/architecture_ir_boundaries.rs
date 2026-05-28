use std::{
    fs,
    path::{Path, PathBuf},
};

use syl_span::SourceId;
use syl_syntax::SourceParser;

#[test]
fn architecture_ir_owners_stay_single_source() {
    let workspace = workspace_root();

    let sema_readme = read_text(&workspace.join("crates/syl_sema/README.md"));
    for required in ["TIR side", "Const MIR", "Map IR"] {
        assert!(
            sema_readme.contains(required),
            "syl_sema README must spell out its IR ownership: {required}"
        );
    }

    let elab_readme = read_text(&workspace.join("crates/syl_elab/README.md"));
    for required in ["Const MIR", "Map IR", "EIR"] {
        assert!(
            elab_readme.contains(required),
            "syl_elab README must document the sema/elab IR boundary: {required}"
        );
    }

    let hw_readme = read_text(&workspace.join("crates/syl_hw/README.md"));
    assert!(
        hw_readme.contains("temporary state"),
        "syl_hw README must prohibit elaboration temporary state from leaking into HW IR"
    );

    let emit_readme = read_text(&workspace.join("crates/syl_emit/README.md"));
    assert!(
        emit_readme.contains("SV AST"),
        "syl_emit README must name the emission-only SV AST owner"
    );

    let elab_const_mir = read_text(&workspace.join("crates/syl_elab/src/const_mir.rs"));
    assert!(
        elab_const_mir.contains("pub(crate) use syl_sema::ir::const_mir"),
        "syl_elab must consume sema-owned Const MIR instead of defining its own copy"
    );
    for forbidden in [
        "struct ConstMirProgram",
        "struct ConstMirBuilder",
        "inner: syl_sema::const_mir::ConstMirProgram",
        "syl_sema::const_mir",
    ] {
        assert!(
            !elab_const_mir.contains(forbidden),
            "syl_elab const_mir boundary must not redefine owner data: {forbidden}"
        );
    }

    let elab_const_eval = read_text(&workspace.join("crates/syl_elab/src/const_eval/mod.rs"));
    assert!(
        elab_const_eval.contains("pub(crate) use syl_sema::ir::const_mir"),
        "syl_elab const_eval owner must source sema-owned const evaluation types"
    );
    assert!(
        elab_const_eval.contains("trait ConstMirElabExt"),
        "syl_elab const_eval owner must carry the elaboration extension trait"
    );
    assert!(
        elab_const_eval.contains("trait ConstValueElaborator"),
        "syl_elab const_eval owner must expose a const elaboration boundary for EIR build"
    );

    let elab_program_lower = read_text(&workspace.join("crates/syl_elab/src/program/lower.rs"));
    assert!(
        elab_program_lower.contains("trait ProgramLoweringInput"),
        "syl_elab program lowering must define a local lowering-input trait"
    );
    assert!(
        elab_program_lower.contains("fn from_input<I>"),
        "syl_elab program lowering must support non-TIR table-backed inputs"
    );
    assert!(
        !elab_program_lower.contains("tir: &'a TirDesign"),
        "ElabProgramBuilder must not own TirDesign directly after introducing ProgramLoweringInput"
    );

    let elab_map_ir = read_text(&workspace.join("crates/syl_elab/src/map_ir.rs"));
    assert!(
        elab_map_ir.contains("pub(crate) use syl_sema::ir::map_ir"),
        "syl_elab must consume sema-owned Map IR instead of defining its own copy"
    );
    for duplicate in [
        "crates/syl_elab/src/map_ir/types.rs",
        "crates/syl_elab/src/map_ir/metrics.rs",
    ] {
        assert!(
            !workspace.join(duplicate).exists(),
            "duplicate Map IR owner path must stay removed: {duplicate}"
        );
    }

    for obsolete in [
        "crates/syl_sema/src/const_eval.rs",
        "crates/syl_sema/src/const_mir.rs",
        "crates/syl_sema/src/map_ir.rs",
        "crates/syl_sema/src/mir.rs",
        "crates/syl_sema/src/mir_type_resolve.rs",
        "crates/syl_sema/src/const_mir/lower.rs",
        "crates/syl_sema/src/const_mir/metrics.rs",
        "crates/syl_sema/src/map_ir/types.rs",
        "crates/syl_sema/src/map_ir/metrics.rs",
    ] {
        assert!(
            !workspace.join(obsolete).exists(),
            "obsolete sema IR path must stay removed: {obsolete}"
        );
    }

    for owned in [
        "crates/syl_sema/src/ir/mod.rs",
        "crates/syl_sema/src/ir/mir.rs",
        "crates/syl_sema/src/ir/mir_type_resolve.rs",
        "crates/syl_sema/src/ir/const_mir/mod.rs",
        "crates/syl_sema/src/ir/const_mir/eval.rs",
        "crates/syl_sema/src/ir/const_mir/lower.rs",
        "crates/syl_sema/src/ir/const_mir/metrics.rs",
        "crates/syl_sema/src/ir/map_ir/mod.rs",
        "crates/syl_sema/src/ir/map_ir/types.rs",
        "crates/syl_sema/src/ir/map_ir/metrics.rs",
    ] {
        assert!(
            workspace.join(owned).exists(),
            "migrated sema IR path must exist: {owned}"
        );
    }

    let eir_source = read_text(&workspace.join("crates/syl_elab/src/eir/design.rs"));
    let eir_assemble = read_text(&workspace.join("crates/syl_elab/src/eir/assemble.rs"));
    assert!(
        eir_assemble.contains("struct EirDesignComposer"),
        "syl_elab must keep EIR assembly separate from the EirDesign data model"
    );
    for forbidden in [
        "EirValidator::new(&modules).validate()?",
        "EirFactCollector::new()",
        "struct Elaborator",
        "EirBuilder",
        "ElabProgram",
        "ConstMirProgram",
        "MapIrProgram",
    ] {
        assert!(
            !eir_source.contains(forbidden),
            "EirDesign data file must not inline builder/checker work: {forbidden}"
        );
    }

    let eir_build_dir = workspace.join("crates/syl_elab/src/eir/build");
    let eir_build = read_rust_tree(&eir_build_dir);
    assert!(
        eir_build.contains("EirRawDesign::new(modules)"),
        "EIR build owner must stop at raw EIR construction"
    );
    assert_no_rust_tree_fragments(
        &eir_build_dir,
        &[
            "EirDesignComposer::compose",
            "EirValidator::new",
            "EirFactCollector::collect",
        ],
        "EIR build owner must not inline validation/facts composition",
    );
    let eir_build_callable =
        read_text(&workspace.join("crates/syl_elab/src/eir/build/callable.rs"));
    assert!(
        eir_build_callable.contains("ConstValueElaborator"),
        "EIR build owner must depend on the local const elaboration boundary"
    );
    assert!(
        !eir_build_callable.contains("ConstMirProgram"),
        "EIR build owner must not own a direct ConstMirProgram dependency once the boundary exists"
    );

    let tir_source = read_text(&workspace.join("crates/syl_sema/src/tir/design.rs"));
    for required in [
        "hir: Arc<HirDesign>",
        "expr_phases: BTreeMap<ExprId, Phase>",
        "expr_types: BTreeMap<ExprId, TypeId>",
        "binding_kinds: BTreeMap<BindingRef, BindingKind>",
        "binding_types: BTreeMap<BindingRef, TypeId>",
    ] {
        assert!(
            tir_source.contains(required),
            "TIR must remain a HIR + side-table design, missing {required}"
        );
    }

    assert_no_rust_tree_fragments(
        &workspace.join("crates/syl_hw/src"),
        &[
            "DriverFacts",
            "ReadFacts",
            "CreateFacts",
            "CellBoundarySummary",
            "driver_facts",
            "read_facts",
            "create_facts",
            "cell_summaries",
            "HwDriveFact",
            "HwReadFact",
            "HwCreateFact",
            "HwCreateKind",
            "HwCellSummary",
            "HwCellSummaryBuilder",
        ],
        "HW IR must not carry elaboration driver metadata",
    );

    assert_no_rust_tree_fragments(
        &workspace.join("crates/syl_elab/src"),
        &[
            "struct ConstMirProgram",
            "struct ConstMirBuilder",
            "struct ConstFunction",
            "enum ConstExpr",
            "enum ConstExprKind",
            "struct MapIrProgram",
            "struct MapIrBuilder",
            "struct MapFunction",
            "enum MapExpr",
        ],
        "syl_elab must not redefine sema-owned Const MIR or Map IR schemas",
    );
}

#[test]
fn architecture_ir_boundaries_expose_debug_dumps() {
    let file = SourceParser::new_in(ir_source(), SourceId::new(0))
        .parse_file()
        .unwrap_or_else(|errors| {
            panic!(
                "ir architecture source must parse:\n{}",
                errors_text(&errors)
            )
        });
    let ast_dump = file.debug_dump();
    assert!(ast_dump.contains("ast "));
    assert!(ast_dump.contains("cell Top"));

    let files = [file];
    let semantic = syl_sema::SemanticCompiler::new();
    let hardware = syl_elab::HardwareCompiler::new();
    let backend = syl_emit::SystemVerilogBackend::new();

    let hir = semantic
        .session(&files)
        .resolve_hir()
        .expect("ir architecture source must resolve HIR");
    let hir_dump = hir.debug_dump();
    assert!(hir_dump.contains("hir "));
    assert!(hir_dump.contains("cell Top"));

    let tir = hir
        .check_tir()
        .expect("ir architecture source must type-check into TIR");
    assert!(tir.debug_dump().contains("tir "));

    let output = hardware.output_for_tir(&tir);
    assert!(
        output.diagnostics().is_empty(),
        "ir architecture source must elaborate without diagnostics:\n{}",
        diagnostics_text(output.diagnostics())
    );

    let const_dump = output
        .const_mir()
        .expect("Const MIR stage must be present")
        .debug_dump();
    assert!(const_dump.contains("const_mir "));
    assert!(const_dump.contains("one("));

    let map_dump = output
        .map_ir()
        .expect("Map IR stage must be present")
        .debug_dump();
    assert!(map_dump.contains("map_ir "));

    let metadata_dump = output
        .metadata()
        .expect("hardware metadata must be present after driver analysis")
        .debug_dump();
    assert!(metadata_dump.contains("hw_metadata "));

    let eir_dump = output
        .eir()
        .expect("EIR stage must be present")
        .debug_dump();
    assert!(eir_dump.contains("eir "));
    assert!(eir_dump.contains("Top"));

    let hwir = output.hwir().expect("HW IR output must be present");
    let hwir_dump = hwir.debug_dump();
    assert!(hwir_dump.contains("hwir "));
    assert!(hwir_dump.contains("Top"));

    let sv_dump = backend
        .debug_dump(hwir)
        .expect("SV AST debug dump must lower from HW IR");
    assert!(sv_dump.contains("sv_ast "));
    assert!(sv_dump.contains("Top"));
}

fn ir_source() -> &'static str {
    r#"
fn one() -> Nat {
    return 1
}

map passthrough<W: Nat>(value: UInt<W>) -> UInt<W> =
    value

cell Top<W: Nat>(
    x: in UInt<W>,
    y: out UInt<W>,
) {
    y := passthrough<W>(x)
}
"#
}

fn diagnostics_text(diagnostics: &[syl_span::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn errors_text(errors: &[syl_span::Diagnostic]) -> String {
    diagnostics_text(errors)
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

fn read_rust_tree(dir: &Path) -> String {
    rust_files_under(dir)
        .into_iter()
        .map(|file| read_text(&file))
        .collect::<Vec<_>>()
        .join("\n")
}

fn assert_no_rust_tree_fragments(dir: &Path, forbidden: &[&str], context: &str) {
    for file in rust_files_under(dir) {
        let text = read_text(&file);
        for fragment in forbidden {
            assert!(
                !text.contains(fragment),
                "{context}: found forbidden fragment `{fragment}` in {}",
                file.display(),
            );
        }
    }
}

fn rust_files_under(dir: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_rust_files(dir, &mut files);
    files.sort();
    files
}

fn walk_rust_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!(
                "failed to read directory entry in {}: {error}",
                dir.display()
            )
        });
        let path = entry.path();
        if path.is_dir() {
            walk_rust_files(&path, files);
            continue;
        }
        if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
