mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_hw::ParametricHwDesign;
use syl_span::SourceId;
use syl_syntax::SourceParser;

struct TestCompiler {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl TestCompiler {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
            backend: SystemVerilogBackend::new(),
        }
    }

    fn compile(&self, source: &str) -> Result<String, String> {
        let hwir = self.compile_hwir(&[source])?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }

    fn compile_sources(&self, sources: &[&str]) -> Result<String, String> {
        let hwir = self.compile_hwir(sources)?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }

    fn compile_hwir(&self, sources: &[&str]) -> Result<ParametricHwDesign, String> {
        let mut files = Vec::new();
        for (idx, source) in sources.iter().enumerate() {
            let file = SourceParser::new_in(source, SourceId::new(idx))
                .parse_file()
                .map_err(|errs| {
                    errs.iter()
                        .map(ToString::to_string)
                        .collect::<Vec<_>>()
                        .join("\n")
                })?;
            files.push(file);
        }
        self.middle
            .compile_files(&files)
            .map_err(|err| err.to_string())
    }
}

#[test]
fn flattens_array_view_connections() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
interface Bus<T> {
    payload: T
    valid: Bit

    view source {
        out payload
        out valid
    }
}

module Child<N: Nat, W: Nat>(bus: out [N] Bus<UInt<W>>.source) {
    bus.payload := 0
    bus.valid := 0
}

module Top<N: Nat, W: Nat>(bus: out [N] Bus<UInt<W>>.source) {
    let child = place Child<N, W>(
        bus: bus,
    )
}
"#,
        )
        .expect("array view formal and actual should flatten consistently");

    assert!(verilog.contains(".bus_payload(bus_payload)"));
    assert!(verilog.contains(".bus_valid(bus_valid)"));
    assert!(!verilog.contains(".bus(bus)"));
}

#[test]
fn local_interface_signal_declares_view_field_nets() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view source {
        out payload
        out valid
        in ready
    }
}

module Top(y: out Bit) {
    signal tmp: Stream<Bit>.source
    tmp.payload := 0
    tmp.valid := 1
    y := tmp.valid
}
"#,
        )
        .expect("local interface signals must lower into field-level nets");

    assert!(verilog.contains("wire tmp_payload;"));
    assert!(verilog.contains("wire tmp_valid;"));
    assert!(verilog.contains("wire tmp_ready;"));
    assert!(!verilog.contains("wire tmp;"));
    assert!(verilog.contains("assign tmp_valid = 1;"));
    assert!(verilog.contains("assign y = tmp_valid;"));
}

#[test]
fn substitutes_interface_array_len_generics() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
interface VecIface<N: Nat, T> {
    payload: [N] T

    view source {
        out payload
    }
}

module Top<W: Nat>(bus: out VecIface<4, UInt<W>>.source) {
    bus.payload := 0
}
"#,
        )
        .expect("interface field array length should use actual generic");

    assert!(verilog.contains("output [((4)*(W))-1:0] bus_payload"));
    assert!(!verilog.contains("((N)*(W))"));
}

#[test]
fn lowers_nested_generic_maps_before_verilog() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
bundle Pair<W: Nat> {
    hi: UInt<W>,
    lo: UInt<W>,
}

map make_pair<W: Nat>(value: UInt<W>) -> Pair<W> =
    Pair<W> {
        hi: value,
        lo: 0,
    }

map high<W: Nat>(pair: Pair<W>) -> UInt<W> =
    pair.hi

map high_from_value<W: Nat>(value: UInt<W>) -> UInt<W> =
    high<W>(make_pair<W>(value))

module Top<W: Nat>(
    value: in UInt<W>,
    y: out UInt<W>,
) {
    y := high_from_value<W>(value)
}
"#,
        )
        .expect("nested generic map calls must lower through middle Map IR before SV emission");

    assert!(verilog.contains("{value, 0}"));
    assert!(!verilog.contains("high_from_value"));
    assert!(!verilog.contains("make_pair"));
}

#[test]
fn inline_cell_reg_width_uses_callsite_bundle_actual_scope() {
    let lib = r#"
package lib.stream

interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view source {
        out payload
        out valid
        in ready
    }

    view sink {
        in payload
        in valid
        out ready
    }
}

cell hold<T, D: Domain>(
    clk: in Clock<D>,
    rst: in Reset<D>,
    up: in Stream<T>.sink,
) -> down: Stream<T>.source {
    reg data: T reset(rst, zero<T>())

    down.valid := up.valid
    down.payload := data
    up.ready := down.ready
    next data := up.payload
}
"#;
    let app = r#"
package app.top

use lib.stream.Stream
use lib.stream.hold

bundle Word<W: Nat> {
    data: UInt<W>,
    last: Bit,
}

module Top<W: Nat, D: Domain>(
    clk: in Clock<D>,
    rst: in Reset<D>,
    up: in Stream<Word<W>>.sink,
    down: out Stream<Word<W>>.source,
) {
    let held = place hold<Word<W>, D>(
        clk: clk,
        rst: rst,
        up: up,
    )

    down.valid := held.valid
    down.payload := held.payload
    held.ready := down.ready
}
"#;

    let verilog = TestCompiler::new()
        .compile_sources(&[lib, app])
        .expect("inline cell generic bundle actual should keep callsite type resolution");

    assert!(
        verilog.contains("reg [(W + 1)-1:0] held_data;"),
        "{verilog}"
    );
    assert!(!verilog.contains("reg held_data;"));
}

#[test]
fn inline_cell_accepts_mixed_named_and_positional_arguments() {
    TestCompiler::new()
        .compile(
            r#"
cell Mix(a: in Bit, b: in Bit, c: in Bit) -> y: Bit {
    y := a
}

module Top(x: in Bit, y: out Bit) {
    let mixed = place Mix(c: x, x, b: x)
    y := mixed
}
"#,
        )
        .expect("inline cell argument binding must handle mixed named and positional forms");
}

#[test]
fn module_generic_defaults_resolve_owner_consts() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
package defaults

const DEFAULT_W: Nat = 7

module Top<
    W: Nat = DEFAULT_W,
>(
    y: out UInt<W>,
) {
    y := 0
}
"#,
        )
        .expect("module generic defaults should lower in owner scope");

    assert!(verilog.contains("parameter W = 7"));
    assert!(!verilog.contains("DEFAULT_W"));
}

#[test]
fn param_dependent_clog2_lowers_to_system_function() {
    let verilog = TestCompiler::new()
        .compile(
            r#"
fn clog2(x: Nat) -> Nat {
    var n: Nat = 0
    var p: Nat = 1

    while p < x {
        p = p << 1
        n = n + 1
    }

    return n
}

module Top<W: Nat>(y: out Bit) {
    const IDX_W: Nat = clog2(W)

    if IDX_W == 0 {
        y := 0
    } else {
        y := 1
    }
}
"#,
        )
        .expect("param-dependent clog2 must lower to valid SystemVerilog");

    assert!(verilog.contains("localparam IDX_W = $clog2(W);"));
    assert!(!verilog.contains("IDX_W = clog2(W);"));
}
