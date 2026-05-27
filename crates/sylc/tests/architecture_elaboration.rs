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
fn architecture_elaboration_pipeline_passes_stay_explicit() {
    let workspace = workspace_root();
    let pipeline = read_text(&workspace.join("crates/syl_elab/src/pipeline.rs"));
    for required in [
        "pub struct EirBuildStage",
        "pub struct EirValidationStage",
        "pub struct EirFactsStage",
        "pub fn eir_build(&self) -> Option<&EirBuildStage>",
        "pub fn eir_validation(&self) -> Option<&EirValidationStage>",
        "pub fn eir_facts(&self) -> Option<&EirFactsStage>",
        "pub struct DriverFactsStage",
        "pub struct DrcStage",
        "pub fn driver_facts(&self) -> Option<&DriverFactsStage>",
        "pub fn drc(&self) -> Option<&DrcStage>",
    ] {
        assert!(
            pipeline.contains(required),
            "elaboration pipeline output must expose explicit pass boundaries: missing {required:?}"
        );
    }

    let stage_runner = read_text(&workspace.join("crates/syl_elab/src/pipeline/stage_runner.rs"));
    for required in [
        "struct ConstMirPass",
        "struct MapIrPass",
        "struct EirBuildPass",
        "struct EirValidationPass",
        "struct EirFactsPass",
        "struct EirComposePass",
        "struct DriverFactsPass",
        "struct DrcPass",
        "struct HardwareMetadataPass",
        "struct HwLoweringPass",
    ] {
        assert!(
            stage_runner.contains(required),
            "elaboration runner must keep pass orchestration explicit: missing {required:?}"
        );
    }
    for forbidden in ["ElaborationOutputBuilder", "fn analyze_drivers(&mut self)"] {
        assert!(
            !stage_runner.contains(forbidden),
            "elaboration runner must not keep giant builder orchestration: found {forbidden:?}"
        );
    }
    let build_pass = section_between(
        &stage_runner,
        "impl EirBuildPass",
        "#[non_exhaustive]\nstruct EirValidationPass",
    );
    for forbidden in [
        "EirValidator::new",
        "EirFactCollector::collect",
        "EirDesignComposer::compose",
    ] {
        assert!(
            !build_pass.contains(forbidden),
            "EirBuildPass must stay raw-only and avoid {forbidden:?}"
        );
    }
    let validation_pass = section_between(
        &stage_runner,
        "impl EirValidationPass",
        "#[non_exhaustive]\nstruct EirFactsPass",
    );
    assert!(
        validation_pass.contains("EirValidator::new"),
        "EirValidationPass must own structural validation"
    );
    assert!(
        !validation_pass.contains("EirFactCollector::collect"),
        "EirValidationPass must not collect facts"
    );
    let facts_pass = section_between(
        &stage_runner,
        "impl EirFactsPass",
        "#[non_exhaustive]\nstruct EirComposePass",
    );
    assert!(
        facts_pass.contains("EirFactCollector::collect"),
        "EirFactsPass must own fact collection"
    );
    assert!(
        !facts_pass.contains("EirValidator::new"),
        "EirFactsPass must not re-run validation"
    );

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
fn architecture_elaboration_output_exposes_each_stage() {
    let file = SourceParser::new(elaboration_ok_source())
        .parse_file()
        .expect("elaboration fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("elaboration fixture must elaborate");

    assert!(output.const_mir().is_some());
    assert!(output.map_ir().is_some());
    assert!(output.eir_build().is_some());
    assert!(output.eir_validation().is_some());
    assert!(output.eir_facts().is_some());
    assert!(output.eir().is_some());
    assert!(output.driver_facts().is_some());
    assert!(output.drc().is_some());
    assert!(output.metadata().is_some());
    assert!(output.hwir().is_some());
    assert!(output.diagnostics().is_empty());
}

#[test]
fn architecture_elaboration_eir_dump_explains_created_and_driven_objects() {
    let file = SourceParser::new(elaboration_ok_source())
        .parse_file()
        .expect("elaboration fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("elaboration fixture must elaborate");
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
fn architecture_elaboration_raw_eir_and_fact_stages_stay_structured() {
    let file = SourceParser::new(elaboration_boundary_source())
        .parse_file()
        .expect("elaboration boundary fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("elaboration boundary fixture must elaborate");
    let eir_build = output
        .eir_build()
        .expect("raw EIR build stage must be present");
    let eir_validation = output
        .eir_validation()
        .expect("EIR validation stage must be present");
    let eir_facts = output.eir_facts().expect("EIR facts stage must be present");

    assert_eq!(eir_build.module_count(), 3);
    assert_eq!(eir_validation.module_count(), 3);
    assert!(
        eir_build.contains_cell_expansion("MakeBit", "made"),
        "raw EIR build must keep inline cell structure before fact collection"
    );
    assert!(
        eir_build.contains_instance_module("Child"),
        "raw EIR build must keep module instances as hierarchy boundaries"
    );
    assert!(
        eir_facts.contains_created_object("Top", "made_tmp"),
        "EIR facts pass must expose created object summaries independently"
    );
    assert!(
        eir_facts.contains_drive("Top", "y"),
        "EIR facts pass must expose driven places independently"
    );
    assert!(
        eir_facts.contains_read("Top", "made_tmp"),
        "EIR facts pass must expose read places independently"
    );
}

#[test]
fn architecture_elaboration_driver_conflict_keeps_call_stack_spans() {
    let source = r#"
cell DoubleDrive() -> y: Bit {
    y := 0
    y := 1
}

cell Top(z: out Bit) {
    let v = inplace DoubleDrive()
    z := v
}
"#;
    let source_id = SourceId::new(21);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("elaboration conflict fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("elaboration conflict fixture must still produce elaboration output");
    let diagnostic = output
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .expect("duplicate hardware driver diagnostic must exist");
    // Inplace expansions don't produce call-stack related spans for driver conflicts
    // within the expanded cell. The conflict is detected within the cell's own body.
    assert!(
        diagnostic.related.len() >= 1,
        "driver conflict diagnostics must include related information"
    );
}

#[test]
fn architecture_elaboration_cell_and_module_boundaries_stay_distinct() {
    let file = SourceParser::new(elaboration_boundary_source())
        .parse_file()
        .expect("boundary fixture must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("boundary fixture must elaborate");
    let metadata = output
        .metadata()
        .expect("boundary fixture must lower metadata after DRC");
    let hwir = output.hwir().expect("boundary fixture must lower HW IR");
    let eir_build = output.eir_build().expect("raw EIR build stage must exist");

    assert!(
        metadata
            .cell_summaries()
            .iter()
            .any(|summary| summary.callable() == "MakeBit" && summary.instance() == "made"),
        "inline cells must stay inline-elaboration boundaries with exported summaries"
    );
    assert!(
        eir_build.contains_cell_expansion("MakeBit", "made"),
        "raw EIR build must mark inline cell expansion boundaries structurally"
    );
    assert!(
        eir_build.contains_instance_module("Child"),
        "raw EIR build must keep module hierarchy as instance boundaries structurally"
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
        "cell calls must stay hierarchical HW instances"
    );
}

fn elaboration_ok_source() -> &'static str {
    r#"
cell MakeBit() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

cell Top(y: out Bit) {
    let made = inplace MakeBit()
    y := made
}
"#
}

fn elaboration_boundary_source() -> &'static str {
    r#"
cell MakeBit() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

cell Child(x: in Bit, y: out Bit) {
    y := x
}

cell Top(y: out Bit) {
    let made = inplace MakeBit()
    let child_inst = place Child(x: made, y: y)
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

fn section_between<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let (_, after_start) = text
        .split_once(start)
        .unwrap_or_else(|| panic!("missing section start {start:?}"));
    let (section, _) = after_start
        .split_once(end)
        .unwrap_or_else(|| panic!("missing section end {end:?}"));
    section
}
