use super::*;
use crate::{
    HardwareCompiler,
    eir::{EirDesignComposer, EirExpr, EirFactCollector, EirRawDesign},
};
use std::sync::Arc;
use syl_sema::{OpaqueSummaryTable, SemanticSession};
use syl_span::Span;
use syl_syntax::SourceParser;

fn compile(source: &str) -> crate::ElaborationOutput {
    let ast = SourceParser::new(source).parse_file().expect(source);
    let hir = SemanticSession::new(&[ast]).resolve_hir().expect(source);
    let tir = hir.check_tir().expect(source);
    HardwareCompiler::new().output_for_tir(&tir)
}
const PORTS: &str = "(clk: in Clock, we: in Bit, re: in Bit, a: in UInt<4>, b: in UInt<4>, data: in UInt<8>) -> y: UInt<8>";
fn check(body: &str, code: Option<&str>) {
    let source = format!("cell Ram{PORTS} {{ mem storage: [16] UInt<8> {body} }}");
    let output = compile(&source);
    if let Some(code) = code {
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|d| d.code.as_deref() == Some(code)),
            "{source}\n{:?}",
            output.diagnostics()
        );
    } else {
        assert!(
            output.diagnostics().is_empty(),
            "{source}\n{:?}",
            output.diagnostics()
        );
        assert!(output.hwir().is_some());
        assert!(
            output
                .metadata()
                .unwrap()
                .create_facts()
                .iter()
                .any(|f| f.kind() == crate::HardwareCreateKind::Memory)
        );
    }
}
#[test]
fn single_simple_dual_and_distributed_ram_lower_with_memory_identity() {
    check(
        "mem_write(storage, clk, we, a, data); mem_read_sync(storage, clk, not we, a, y)",
        None,
    );
    check(
        "mem_write(storage, clk, we, a, data); mem_read_sync(storage, clk, re, b, y)",
        None,
    );
    check(
        "mem_write(storage, clk, we, a, data); y := storage[b]",
        None,
    );
}
#[test]
fn overlapping_write_ports_report_both_origins() {
    let source = format!(
        "cell Ram{PORTS} {{ mem storage: [16] UInt<8> mem_write(storage, clk, we, a, data); mem_write(storage, clk, re, b, data); y := storage[b] }}"
    );
    let output = compile(&source);
    let diagnostic = output
        .diagnostics()
        .iter()
        .find(|d| d.code.as_deref() == Some("E_MIDDLE_DUPLICATE_MEM_WRITE_PORT"))
        .expect("two write ports must conflict");
    assert_eq!(diagnostic.related.len(), 2);
    assert_ne!(diagnostic.related[0].span, diagnostic.related[1].span);
}
#[test]
fn mutually_exclusive_elaboration_branches_can_select_write_ports() {
    let source = format!(
        "cell Ram<SELECT: bool>{PORTS} {{ mem storage: [16] UInt<8> if SELECT {{ mem_write(storage, clk, we, a, data); }} else {{ mem_write(storage, clk, we, b, data); }} y := storage[b] }}"
    );
    let output = compile(&source);
    assert!(
        output.diagnostics().is_empty(),
        "{:?}",
        output.diagnostics()
    );
}
#[test]
fn sync_read_targets_use_ordinary_single_driver_checks() {
    check(
        "mem_write(storage, clk, we, a, data); mem_read_sync(storage, clk, re, b, y); y := data",
        Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"),
    );
    check(
        "mem_write(storage, clk, we, a, data); mem_read_sync(storage, clk, re, b, y); mem_read_sync(storage, clk, re, a, y)",
        Some("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"),
    );
}
#[test]
fn dual_clock_memory_and_cross_domain_controls_are_rejected() {
    for read in [
        "mem_read_sync(storage, rclk, re, b, y);",
        "reg q: UInt<8> reset(rst, 0) next q := storage[b] y := q",
    ] {
        let source = format!(
            "cell Ram<W: Domain, R: Domain>(wclk: in Clock<W>, rclk: in Clock<R>, rst: in Reset<R>, we: in Bit, re: in Bit, a: in UInt<4>, b: in UInt<4>, data: in UInt<8>) -> y: UInt<8> {{ mem storage: [16] UInt<8> mem_write(storage, wclk, we, a, data); {read} }}"
        );
        let output = compile(&source);
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION")),
            "{:?}",
            output.diagnostics()
        );
    }
    for operand in ["q, a, data", "we, qaddr, data", "we, a, qdata"] {
        let source = format!(
            "cell Ram<W: Domain, R: Domain>(wclk: in Clock<W>, rclk: in Clock<R>, rst: in Reset<R>, we: in Bit, a: in UInt<4>, data: in UInt<8>) -> y: UInt<8> {{ reg q: Bit reset(rst, 0) reg qaddr: UInt<4> reset(rst, 0) reg qdata: UInt<8> reset(rst, 0) next q := we next qaddr := a next qdata := data mem storage: [16] UInt<8> mem_write(storage, wclk, {operand}); y := storage[a] }}"
        );
        let output = compile(&source);
        assert!(
            output
                .diagnostics()
                .iter()
                .any(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION")),
            "{:?}",
            output.diagnostics()
        );
    }
}
#[test]
fn drc_defensively_rejects_nonmemory_objects() {
    let item = EirItem::MemoryWrite {
        clock: EirExpr::ident("clk"),
        memory: "signal".into(),
        enable: EirExpr::Int(1),
        addr: EirExpr::Int(0),
        data: EirExpr::Int(0),
        reads: Vec::new(),
        origin: EirOrigin::new(Span::new(0, 1), Vec::new()),
    };
    let raw = Arc::new(EirRawDesign::new(vec![EirModule::new(
        "Top",
        vec![],
        vec![],
        vec![item],
    )]));
    let facts =
        Arc::new(EirFactCollector::collect(raw.modules(), &OpaqueSummaryTable::new()).unwrap());
    let eir = EirDesignComposer::compose(raw, facts);
    let errors = MemoryPortDrcChecker::new(&eir).collect_errors();
    assert_eq!(errors.len(), 1);
    assert!(errors[0].to_string().contains("signal"));
}

#[test]
fn declared_memory_bridge_only_trusts_its_named_data_path() {
    let body = "cell Ram(wclk: in Clock, rclk: in Clock, we: in Bit, re: in Bit, a: in UInt<4>, b: in UInt<4>, data: in UInt<8>, unrelated: in UInt<8>, other: out UInt<8>) -> y: UInt<8> { mem storage: [16] UInt<8> mem_write(storage, wclk, we, a, data); mem_read_sync(storage, rclk, re, b, y); other := 0 }";
    for (input, output, good) in [
        ("data", "y", true),
        ("unrelated", "y", false),
        ("data", "other", false),
    ] {
        let result = compile(&format!("@cdc_sync({input}, {output}, rclk) {body}"));
        if good {
            assert!(
                result.diagnostics().is_empty(),
                "{:?}",
                result.diagnostics()
            );
        } else {
            assert!(
                result
                    .diagnostics()
                    .iter()
                    .any(|d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION")),
                "{:?}",
                result.diagnostics()
            );
        }
    }
}

#[test]
fn combinational_memory_reads_can_flow_through_let_bindings() {
    for read in [
        "let word = storage[b]; y := word",
        "let word: UInt<8> = storage[b]; y := word",
        "let word = storage[b]; let forwarded = word; y := forwarded",
        "let word = storage[b]; let masked = word and data; y := masked",
    ] {
        check(
            &format!("mem_write(storage, clk, we, a, data); {read}"),
            None,
        );
        let output = compile(&format!(
            "cell Ram{PORTS} {{ mem storage: [16] UInt<8> mem_write(storage, clk, we, a, data); {read} }}"
        ));
        let facts = output.eir_facts().unwrap();
        assert!(facts.contains_read("Ram", "storage"));
        assert!(facts.contains_read("Ram", "b"));
        assert!(!facts.contains_read("Ram", "idx(storage,b)"));
    }
}

#[test]
fn memory_intrinsics_accept_parenthesized_memory_identifiers() {
    for memory in ["storage", "(storage)", "(((storage)))"] {
        check(
            &format!(
                "mem_write({memory}, clk, we, a, data); mem_read_sync({memory}, clk, re, b, y)"
            ),
            None,
        );
    }
}

#[test]
fn let_bound_memory_reads_preserve_cdc_checks_on_the_address() {
    let output = compile(
        "cell Ram<A: Domain, B: Domain>(ca: in Clock<A>, ra: in Reset<A>, cb: in Clock<B>, rb: in Reset<B>, we: in Bit, addr: in UInt<4>, data: in UInt<8>) -> y: UInt<8> { reg qa: UInt<4> reset(ra, 0) next qa := addr mem storage: [16] UInt<8> mem_write(storage, cb, we, addr, data); let word = storage[qa]; reg qb: UInt<8> reset(rb, 0) next qb := word y := qb }",
    );
    assert!(output.eir_facts().unwrap().contains_read("Ram", "qa"));
    assert!(
        output.diagnostics().iter().any(
            |d| d.code.as_deref() == Some("E_MIDDLE_CDC_VIOLATION") && d.message.contains("qa")
        ),
        "{:?}",
        output.diagnostics()
    );
}
