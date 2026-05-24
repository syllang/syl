mod support;

use support::MiddleCompiler;
use syl_elab::ElaborationOutput;
use syl_hw::HwPlace;
use syl_span::{SourceId, Span};
use syl_syntax::SourceParser;

struct StaticFactHarness {
    middle: MiddleCompiler,
}

impl StaticFactHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
        }
    }

    fn compile_output(&self, source: &str) -> Result<ElaborationOutput, String> {
        let file = SourceParser::new(source).parse_file().map_err(|errs| {
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        self.middle
            .output_files(&[file])
            .map_err(|err| err.to_string())
    }
}

#[test]
fn instance_input_expression_read_fact_uses_object_place() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
extern module UseBit(x: in Bit)

module Top(a: in Bit, y: out Bit) {
    inst use_bit = UseBit(x: a and 1)
    y := 0
}
"#,
        )
        .expect("instance input expression should compile");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let top_reads: Vec<_> = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .collect();

    assert!(
        top_reads
            .iter()
            .any(|fact| matches!(fact.source_place(), HwPlace::Object { name, .. } if name == "a"))
    );
    assert!(
        !top_reads
            .iter()
            .any(|fact| matches!(fact.source_place(), HwPlace::Expr(_)))
    );
}

#[test]
fn extern_module_out_port_records_driver_fact() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
extern module DriveBit(y: out Bit)

module Top(y: out Bit) {
    inst drive_bit = DriveBit(y: y)
}
"#,
        )
        .expect("extern module out port should be represented by port-direction facts");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    assert!(metadata.driver_facts().iter().any(|fact| {
        fact.module() == "Top"
            && (matches!(fact.target_place(), HwPlace::Object { name, .. } if name == "y")
                || matches!(fact.target_place(), HwPlace::Ident(name) if name == "y"))
    }));
}

#[test]
fn inline_cell_result_aggregate_assign_uses_result_object_identity() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
bundle Pair {
    lo: Bit,
    hi: Bit,
}

cell MakePair() -> y: Pair {
    y := Pair {
        lo: 1,
        hi: 0,
    }
}

module Top(y: out Pair) {
    alias made = MakePair()
    y := made
}
"#,
        )
        .expect("cell result aggregate assignment should target the inlined result object");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    assert!(metadata.driver_facts().iter().any(|fact| {
        matches!(fact.target_place(), HwPlace::Object { name, .. } if name == "made")
    }));
}

#[test]
fn duplicate_driver_diagnostic_has_primary_and_related_origins() {
    let source = r#"
module Bad(y: out Bit) {
    y := 0
    y := 1
}
"#;
    let source_id = SourceId::new(12);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let output = MiddleCompiler::new()
        .output_files(&[file])
        .expect("duplicate driver fixture must produce elaboration output");
    let diagnostics = output.diagnostics();
    let diagnostic = diagnostics
        .iter()
        .find(|diagnostic| diagnostic.code.as_deref() == Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"))
        .expect("duplicate driver diagnostic must be reported");
    let first_start = source
        .find("y := 0")
        .expect("test fixture must contain first drive");
    let second_start = source
        .find("y := 1")
        .expect("test fixture must contain second drive");

    assert_eq!(
        diagnostic.span,
        Span::new_in(source_id, second_start, second_start + "y := 1".len())
    );
    assert!(
        diagnostic
            .related
            .iter()
            .any(|related| related.span.start == first_start)
    );
    assert!(
        diagnostic
            .related
            .iter()
            .any(|related| related.span.start == second_start)
    );
}
