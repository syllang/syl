mod support;

use std::{
    fs,
    path::{Path, PathBuf},
};

use support::MiddleCompiler;
use syl_hw::{HwItem, ParametricHwItem};
use syl_span::SourceId;
use syl_syntax::SourceParser;

#[test]
fn architecture_phase4_pipeline_passes_stay_explicit() {
    let workspace = workspace_root();
    let pipeline = read_text(&workspace.join("crates/syl_elab/src/pipeline.rs"));
    for required in [
        "pub struct DriverFactsStage",
        "pub struct DrcStage",
        "pub fn driver_facts(&self) -> Option<&DriverFactsStage>",
        "pub fn drc(&self) -> Option<&DrcStage>",
    ] {
        assert!(
            pipeline.contains(required),
            "Phase 4 pipeline output must expose explicit pass boundaries: missing {required:?}"
        );
    }

    let stage_runner = read_text(&workspace.join("crates/syl_elab/src/pipeline/stage_runner.rs"));
    for required in [
        "struct ConstMirPass",
        "struct MapIrPass",
        "struct EirBuildPass",
        "struct DriverFactsPass",
        "struct DrcPass",
        "struct HardwareMetadataPass",
        "struct HwLoweringPass",
    ] {
        assert!(
            stage_runner.contains(required),
            "Phase 4 runner must keep pass orchestration explicit: missing {required:?}"
        );
    }
    for forbidden in ["ElaborationOutputBuilder", "fn analyze_drivers(&mut self)"] {
        assert!(
            !stage_runner.contains(forbidden),
            "Phase 4 runner must not keep giant builder orchestration: found {forbidden:?}"
        );
    }

    let facts = normalize_whitespace(&read_text(
        &workspace.join("crates/syl_elab/src/driver/facts.rs"),
    ));
    let drc = normalize_whitespace(&read_text(
        &workspace.join("crates/syl_elab/src/driver/drc.rs"),
    ));
    assert!(
        facts.contains("struct DriverFactsCollector"),
        "driver facts collection must be a first-class pass"
    );
    assert!(
        drc.contains("struct DriverDrcChecker"),
        "driver DRC must be a first-class pass"
    );
    assert!(
        drc.contains("facts.drives()"),
        "driver DRC must consume collected driver facts rather than builder side effects"
    );
}

#[test]
fn architecture_phase4_output_exposes_each_stage() {
    let file = SourceParser::new(phase4_ok_source())
        .parse_file()
        .expect("phase4 fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("phase4 fixture must elaborate");

    assert!(output.const_mir().is_some());
    assert!(output.map_ir().is_some());
    assert!(output.eir().is_some());
    assert!(output.driver_facts().is_some());
    assert!(output.drc().is_some());
    assert!(output.metadata().is_some());
    assert!(output.hwir().is_some());
    assert!(output.diagnostics().is_empty());
}

#[test]
fn architecture_phase4_eir_dump_explains_created_and_driven_objects() {
    let file = SourceParser::new(phase4_ok_source())
        .parse_file()
        .expect("phase4 fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("phase4 fixture must elaborate");
    let dump = output
        .eir()
        .expect("EIR stage must be present")
        .debug_dump();

    for required in [
        "create signal Top.made_tmp",
        "drive Top y kind=continuous",
        "read Top made_tmp guard=root",
        "origin=",
    ] {
        assert!(
            dump.contains(required),
            "EIR dump must explain create/drive/read provenance: missing {required:?}\n{dump}"
        );
    }
}

#[test]
fn architecture_phase4_driver_conflict_keeps_call_stack_spans() {
    let source = r#"
cell DoubleDrive() -> y: Bit {
    y := 0
    y := 1
}

module Top(z: out Bit) {
    alias v = DoubleDrive()
    z := v
}
"#;
    let source_id = SourceId::new(21);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("phase4 conflict fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("phase4 conflict fixture must still produce elaboration output");
    let diagnostic = output
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .expect("duplicate hardware driver diagnostic must exist");
    let call_start = source
        .find("alias v = DoubleDrive()")
        .map(|start| start + "alias v = ".len())
        .expect("fixture must contain inline cell callsite");

    assert!(
        diagnostic
            .related
            .iter()
            .any(|related| related.span.start == call_start),
        "driver conflict diagnostics must include the elaboration call stack callsite"
    );
}

#[test]
fn architecture_phase4_cell_and_module_boundaries_stay_distinct() {
    let file = SourceParser::new(phase4_boundary_source())
        .parse_file()
        .expect("boundary fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("boundary fixture must elaborate");
    let metadata = output
        .metadata()
        .expect("boundary fixture must lower metadata after DRC");
    let hwir = output.hwir().expect("boundary fixture must lower HW IR");
    let eir_dump = output.eir().expect("EIR stage must exist").debug_dump();

    assert!(
        metadata
            .cell_summaries()
            .iter()
            .any(|summary| summary.callable() == "MakeBit" && summary.instance() == "made"),
        "inline cells must stay inline-elaboration boundaries with exported summaries"
    );
    assert!(
        eir_dump.contains("cell inline MakeBit as made"),
        "EIR dump must mark inline cell expansion boundaries"
    );
    assert!(
        eir_dump.contains("instance Child as child_inst"),
        "EIR dump must keep module hierarchy as instance boundaries"
    );

    let top = hwir
        .modules()
        .iter()
        .find(|module| module.name() == "Top")
        .expect("Top module must be present");
    assert!(
        top.items().iter().any(|item| matches!(
            item,
            ParametricHwItem::Core {
                item: HwItem::Instance(instance),
                ..
            } if instance.module() == "Child"
        )),
        "module calls must stay hierarchical HW instances"
    );
}

fn phase4_ok_source() -> &'static str {
    r#"
cell MakeBit() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

module Top(y: out Bit) {
    alias made = MakeBit()
    y := made
}
"#
}

fn phase4_boundary_source() -> &'static str {
    r#"
cell MakeBit() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

module Child(x: in Bit, y: out Bit) {
    y := x
}

module Top(y: out Bit) {
    alias made = MakeBit()
    inst child_inst = Child(x: made, y: y)
}
"#
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

fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}
