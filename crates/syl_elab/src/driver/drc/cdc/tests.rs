use crate::{ElaborationOutput, HardwareCompiler};
use syl_sema::{
    OpaqueItemKind, OpaqueItemSummary, OpaqueSummaryTable, SemanticSession, SummaryCdcCrossing,
};
use syl_syntax::SourceParser;

pub(crate) fn compile(source: &str, summaries: OpaqueSummaryTable) -> ElaborationOutput {
    let ast = SourceParser::new(source).parse_file().expect(source);
    let hir = SemanticSession::new(&[ast]).resolve_hir().expect(source);
    let tir = hir.check_tir().expect(source);
    HardwareCompiler::with_opaque_summaries(summaries).output_for_tir(&tir)
}
pub(crate) fn diagnostics(source: &str) -> ElaborationOutput {
    compile(source, OpaqueSummaryTable::new())
}
pub(crate) fn rejects(source: &str, code: &str) -> ElaborationOutput {
    let output = diagnostics(source);
    assert!(
        output
            .diagnostics()
            .iter()
            .any(|d| d.code.as_deref() == Some(code)),
        "{source}\n{:?}",
        output.diagnostics()
    );
    output
}
pub(crate) fn accepts(source: &str) -> ElaborationOutput {
    let output = diagnostics(source);
    assert!(
        output.diagnostics().is_empty(),
        "{source}\n{:?}",
        output.diagnostics()
    );
    assert!(output.hwir().is_some());
    output
}
const DUAL: &str = "<A: Domain, B: Domain>(ca: in Clock<A>, ra: in Reset<A>, cb: in Clock<B>, rb: in Reset<B>, x: in Bit) -> y: Bit";

#[test]
fn local_direct_and_multihop_crossing_keep_source_origin() {
    for path in ["qa", "hop2"] {
        let source = format!(
            "cell Top{DUAL} {{ reg qa: Bit reset(ra, 0) reg qb: Bit reset(rb, 0) signal hop1: Bit := qa signal hop2: Bit := hop1 next qa := x next qb := {path} y := qb }}"
        );
        let output = rejects(&source, "E_MIDDLE_CDC_VIOLATION");
        let errors = output
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION"))
            .collect::<Vec<_>>();
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0]
                .related
                .iter()
                .any(|r| source[r.span.start..r.span.end].contains("qa")),
            "{:?}",
            errors[0]
        );
    }
}

const STAGES: &str = r#"
cell Producer(clk: in Clock, rst: in Reset, x: in Bit) -> y: Bit {
    reg q: Bit reset(rst, 0) next q := x y := q
}
cell Pass(x: in Bit) -> y: Bit { y := x }
cell Consumer(clk: in Clock, rst: in Reset, x: in Bit) -> y: Bit {
    reg q: Bit reset(rst, 0) next q := x y := q
}
"#;
#[test]
fn hierarchical_launch_sample_and_comb_child_propagate() {
    for value in ["p", "passthrough"] {
        let source = format!(
            "{STAGES} cell Top{DUAL} {{ let p = place Producer(clk: ca, rst: ra, x: x) let passthrough = place Pass(x: p) let c = place Consumer(clk: cb, rst: rb, x: {value}) y := c }}"
        );
        let out = rejects(&source, "E_MIDDLE_CDC_VIOLATION");
        assert_eq!(out.diagnostics().len(), 1, "{:?}", out.diagnostics());
    }
}
const BRIDGE: &str = r#"
@cdc_sync(a, b, clk)
cell Bridge(clk: in Clock, rst: in Reset, a: in Bit) -> b: Bit {
    reg one: Bit reset(rst, 0) reg two: Bit reset(rst, 0)
    next one := a next two := one b := two
}
"#;
#[test]
fn source_bridge_switches_domain_and_rejects_return_to_source_domain() {
    let body = "let p = place Producer(clk: ca, rst: ra, x: x) let bridge = place Bridge(clk: cb, rst: rb, a: p) let c = place Consumer(clk: cb, rst: rb, x: bridge) y := c";
    accepts(&format!("{STAGES} {BRIDGE} cell Top{DUAL} {{ {body} }}"));
    rejects(
        &format!(
            "{STAGES} {BRIDGE} cell Top{DUAL} {{ {} }}",
            body.replace("Consumer(clk: cb, rst: rb", "Consumer(clk: ca, rst: ra")
        ),
        "E_MIDDLE_CDC_VIOLATION",
    );
    rejects(
        &format!(
            "{STAGES} {BRIDGE} cell Top{DUAL} {{ {} }}",
            body.replace("place Bridge", "inplace Bridge")
        ),
        "E_MIDDLE_INPLACE_ON_CDC_SYNC",
    );
}
#[test]
fn host_summary_marks_extern_bridge_without_source_attribute() {
    let source = format!(
        "{STAGES} extern cell Vendor(clk: in Clock, a: in Bit) -> b: Bit cell Top{DUAL} {{ let p = place Producer(clk: ca, rst: ra, x: x) let bridge = place Vendor(clk: cb, a: p) let c = place Consumer(clk: cb, rst: rb, x: bridge) y := c }}"
    );
    rejects(&source, "E_MIDDLE_CDC_VIOLATION");
    let mut summaries = OpaqueSummaryTable::new();
    summaries.register(
        OpaqueItemSummary::builder(OpaqueItemKind::ExternCell, "Vendor")
            .cdc_crossing(SummaryCdcCrossing::new("a", "b", "clk"))
            .build(),
    );
    let output = compile(&source, summaries.clone());
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
    let reverse = compile(
        &source.replace("Consumer(clk: cb, rst: rb", "Consumer(clk: ca, rst: ra"),
        summaries,
    );
    assert!(
        reverse
            .diagnostics()
            .iter()
            .any(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION"))
    );
}
#[test]
fn reset_domain_conflicts_and_single_clock_compatibility() {
    rejects(
        "cell Bad<A: Domain, B: Domain>(clk: in Clock<A>, rst: in Reset<B>) { reg q: Bit reset(rst, 0) next q := 0 }",
        "E_MIDDLE_RESET_DOMAIN_MISMATCH",
    );
    rejects(
        "cell Bad(clk: in Clock, rst: in Reset<Async, ActiveLow>) { reg q: Bit reset(rst, 0, sync) next q := 0 }",
        "E_MIDDLE_RESET_CONFIG_CONFLICT",
    );
    accepts(
        "cell Good(clk: in Clock, rst: in Reset, x: in Bit) -> y: Bit { reg q: Bit reset(rst, 0) next q := x y := q }",
    );
    accepts(
        "cell Good<D: Domain>(clk: in Clock<D>, rst: in Reset, x: in Bit) -> y: Bit { reg q: Bit reset(rst, 0) next q := x y := q }",
    );
}
#[test]
fn clocked_assertions_and_reset_values_sample_data_domains() {
    rejects(
        &format!(
            "cell Top{DUAL} {{ reg qa: Bit reset(ra, 0) reg qb: Bit reset(rb, qa) next qa := x next qb := 0 y := qb }}"
        ),
        "E_MIDDLE_CDC_VIOLATION",
    );
    rejects(
        &format!(
            "{STAGES} cell Check(clk: in Clock, x: in Bit) {{ assert(x); }} cell Top{DUAL} {{ let p = place Producer(clk: ca, rst: ra, x: x) let check = place Check(clk: cb, x: p) y := p }}"
        ),
        "E_MIDDLE_CDC_VIOLATION",
    );
}
