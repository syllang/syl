mod support;

use support::MiddleCompiler;
use syl_elab::ElaborationOutput;
use syl_hw::HwPlace;
use syl_sema::{
    BackendConstraint, OpaqueItemKind, OpaqueItemSummary, OpaqueSummaryTable, SummaryCapability,
    SummaryDirection, SummaryEndpoint, SummaryLatencyClass, SummaryLayout, SummaryPath,
    TrustBoundary,
};
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

    fn with_opaque_summaries(opaque_summaries: OpaqueSummaryTable) -> Self {
        Self {
            middle: MiddleCompiler::with_opaque_summaries(opaque_summaries),
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
extern cell UseBit(x: in Bit)

cell Top(a: in Bit, y: out Bit) {
    let use_bit = place UseBit(x: a and 1)
    y := 0
}
"#,
        )
        .expect("instance input expression should compile");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| panic!("successful elaboration must expose hardware metadata: {:?}", output.diagnostics()));

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
fn extension_map_read_facts_use_expanded_receiver_fields() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
interface Stage<T> {
    payload: T
    valid: Bit
    ready: Bit

    view tap {
        in payload
        in valid
        in ready
    }
}

map fire<T>(this stage: Stage<T>.tap) -> Bit =
    stage.valid and stage.ready

cell Top(stage: in Stage<Bit>.tap, y: out Bit) {
    y := stage.fire()
}
"#,
        )
        .expect("extension map read facts should compile");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: {:?}",
                output.diagnostics()
            )
        });

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "stage_valid"));
    assert!(top_reads.iter().any(|read| read == "stage_ready"));
    assert!(!top_reads.iter().any(|read| read == "stage"));
}

#[test]
fn extern_module_out_port_records_driver_fact() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
extern cell DriveBit(y: out Bit)

cell Top(y: out Bit) {
    let drive_bit = place DriveBit(y: y)
}
"#,
        )
        .expect("extern cell out port should be represented by port-direction facts");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: {:?}",
                output.diagnostics()
            )
        });

    assert!(metadata.driver_facts().iter().any(|fact| {
        fact.module() == "Top"
            && (matches!(fact.target_place(), HwPlace::Object { name, .. } if name == "y")
                || matches!(fact.target_place(), HwPlace::Ident(name) if name == "y"))
    }));

    let summary = metadata
        .opaque_summaries()
        .get("DriveBit")
        .expect("extern cell summary must be exported into compilation metadata");
    assert!(matches!(summary.kind(), OpaqueItemKind::ExternCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::SourceDerived
    ));
    assert_eq!(
        summary
            .driven_fields()
            .iter()
            .map(SummaryPath::display)
            .collect::<Vec<_>>(),
        vec!["y".to_string()]
    );
}

#[test]
fn trusted_precompiled_summary_overrides_boundary_metadata() {
    let summary = OpaqueItemSummary::builder(OpaqueItemKind::PrecompiledCell, "VendorDrive")
        .endpoint(SummaryEndpoint::new(
            "y",
            SummaryDirection::Out,
            SummaryLayout::Bit,
            SummaryCapability::Value,
        ))
        .driven_field(SummaryPath::new("y"))
        .latency_class(SummaryLatencyClass::Sequential)
        .trust_boundary(TrustBoundary::VendorBlackBox {
            vendor: "acme".to_string(),
        })
        .backend_constraint(BackendConstraint::RequiresBackend {
            backend: "systemverilog".to_string(),
        })
        .build();
    let table = OpaqueSummaryTable::from_iter([summary]);
    let output = StaticFactHarness::with_opaque_summaries(table)
        .compile_output(
            r#"
extern cell VendorDrive(y: out Bit)

cell Top(y: out Bit) {
    let vendor = place VendorDrive(y: y)
}
"#,
        )
        .expect("trusted precompiled summary must compile via extern stub boundary");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: {:?}",
                output.diagnostics()
            )
        });
    let summary = metadata
        .opaque_summaries()
        .get("VendorDrive")
        .expect("merged opaque summary must be preserved in metadata");

    assert!(matches!(summary.kind(), OpaqueItemKind::PrecompiledCell));
    assert!(matches!(
        summary.trust_boundary(),
        TrustBoundary::VendorBlackBox { vendor } if vendor == "acme"
    ));
    assert_eq!(summary.latency_class(), SummaryLatencyClass::Sequential);
    assert!(matches!(
        summary.backend_constraints(),
        [BackendConstraint::RequiresBackend { backend }] if backend == "systemverilog"
    ));
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

cell Top(y: out Pair) {
    let made = place MakePair()
    y := made
}
"#,
        )
        .expect("cell result aggregate assignment should target the inlined result object");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: {:?}",
                output.diagnostics()
            )
        });

    assert!(metadata.driver_facts().iter().any(|fact| {
        matches!(fact.target_place(), HwPlace::Object { name, .. } if name == "made")
    }));
}

#[test]
fn software_mutable_local_controls_elaboration_read_selection() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
cell Top(a: in Bit, b: in Bit, y: out Bit) {
    var choose_b: bool = false
    choose_b = true

    if choose_b {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("software-only mutable locals should select elaboration branches without becoming hardware values");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: {:?}",
                output.diagnostics()
            )
        });

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "a"));
    assert!(!top_reads.iter().any(|read| read == "choose_b"));
}

#[test]
fn mutable_local_assignment_inside_if_flows_to_later_reads() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
cell Top(a: in Bit, b: in Bit, y: out Bit) {
    var selected: bool = false

    if true {
        selected = true
    }

    if selected {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("if-local mutation must remain visible after the branch");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: output={output:?}, diagnostics={:?}",
                output.diagnostics()
            )
        });

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "a"));
}

#[test]
fn mutable_local_assignment_inside_for_flows_to_later_reads() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
cell Top(a: in Bit, b: in Bit, y: out Bit) {
    var count = 0

    for i in 0..2 {
        count = count + 1
    }

    if count == 2 {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("for-local mutation must remain visible after the loop");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "a"));
}

#[test]
fn software_struct_field_assign_stays_on_software_path() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
struct Config {
    enabled: bool,
}

cell Top(a: in Bit, b: in Bit, y: out Bit) {
    var cfg = Config { enabled: false }
    cfg.enabled = true

    if cfg.enabled {
        y := b
    } else {
        y := a
    }
}
"#,
        );
    let output = output.unwrap_or_else(|err| {
        panic!(
            "software struct field assignment must lower without using bundle hardware paths: {err}"
        )
    });
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: output={output:?}, diagnostics={:?}",
                output.diagnostics()
            )
        });

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "a"));
    assert!(!top_reads.iter().any(|read| read.contains("cfg")));
}

#[test]
fn explicit_struct_typed_mutable_local_stays_on_software_path() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
struct Config {
    enabled: bool,
}

cell Top(a: in Bit, b: in Bit, y: out Bit) {
    var cfg: Config = Config { enabled: false }
    cfg.enabled = true

    if cfg.enabled {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("explicitly typed software struct mutable local must support field mutation and later reads");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: output={output:?}, diagnostics={:?}",
                output.diagnostics()
            )
        });

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "a"));
    assert!(!top_reads.iter().any(|read| read.contains("cfg")));
}

#[test]
fn software_struct_field_assign_updates_later_whole_value_uses() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
struct Config {
    enabled: bool,
}

cell Top(a: in Bit, b: in Bit, y: out Bit) {
    var cfg = Config { enabled: false }
    cfg.enabled = true

    var forwarded = cfg
    if forwarded.enabled {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("field assignment must rebuild the root binding for later whole-value uses");
    let metadata = output
        .metadata()
        .unwrap_or_else(|| {
            panic!(
                "successful elaboration must expose hardware metadata: output={output:?}, diagnostics={:?}",
                output.diagnostics()
            )
        });

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "a"));
    assert!(!top_reads.iter().any(|read| read.contains("cfg")));
    assert!(!top_reads.iter().any(|read| read.contains("forwarded")));
}

#[test]
fn unknown_const_if_merges_visible_mutations_conservatively() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
cell Top<ENABLE: bool>(a: in Bit, b: in Bit, y: out Bit) {
    var selected: bool = false

    if ENABLE {
        selected = true
    }

    if selected {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("unknown const-if should still elaborate");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "a"));
    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "selected"));
}

#[test]
fn symbolic_for_merges_visible_mutations_conservatively() {
    let output = StaticFactHarness::new()
        .compile_output(
            r#"
cell Top<ENABLE: bool>(a: in Bit, b: in Bit, y: out Bit) {
    var count: nat = 0

    for i in 0..1 {
        if ENABLE {
            count = count + 1
        }
    }

    if count == 1 {
        y := b
    } else {
        y := a
    }
}
"#,
        )
        .expect("symbolic for should still elaborate");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let top_reads = metadata
        .read_facts()
        .iter()
        .filter(|fact| fact.module() == "Top")
        .map(|fact| fact.source_place().display())
        .collect::<Vec<_>>();

    assert!(top_reads.iter().any(|read| read == "a"));
    assert!(top_reads.iter().any(|read| read == "b"));
    assert!(!top_reads.iter().any(|read| read == "count"));
}

#[test]
fn duplicate_driver_diagnostic_has_primary_and_related_origins() {
    let source = r#"
cell Bad(y: out Bit) {
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
