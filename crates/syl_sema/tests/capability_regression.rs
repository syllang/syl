use syl_elab::MiddleCompiler;
use syl_span::{SourceId, Span};
use syl_syntax::SourceParser;

struct CapabilityHarness {
    middle: MiddleCompiler,
}

impl CapabilityHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
        }
    }

    fn check(&self, source: &str) -> Result<(), String> {
        self.compile_sources(&[source])
    }

    fn compile_sources(&self, sources: &[&str]) -> Result<(), String> {
        let mut files = Vec::new();
        for source in sources {
            files.push(Self::parse_source(source)?.file);
        }
        self.middle
            .compile_files(&files)
            .map(|_| ())
            .map_err(|err| err.to_string())
    }

    fn parse_source(source: &str) -> Result<syl_syntax::ParseOutput, String> {
        let output = SourceParser::new(source).parse_file_partial();
        if output.diagnostics.is_empty() {
            Ok(output)
        } else {
            Err(output
                .diagnostics
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n"))
        }
    }
}

#[test]
fn rejects_write_only_view_field_through_array_projection() {
    let err = CapabilityHarness::new()
        .check(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}

module Bad(up: in [2] Stream<Bit>.sink, y: out Bit) {
    y := up[0].ready
}
"#,
        )
        .expect_err("array projection must not hide write-only view fields");

    assert!(err.contains("up[0].ready is not readable"));
}

#[test]
fn unreadable_view_field_diagnostic_uses_expression_span() {
    let source = r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}

module Bad(up: in [2] Stream<Bit>.sink, y: out Bit) {
    y := up[0].ready
}
"#;
    let source_id = SourceId::new(9);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let output = MiddleCompiler::new().session(&[file]).check();
    let diagnostic = output
        .diagnostics()
        .first()
        .expect("capability error must be reported");
    let start = source
        .find("up[0].ready")
        .expect("test fixture must contain unreadable place");

    assert_eq!(
        diagnostic.span,
        Span::new_in(source_id, start, start + "up[0].ready".len())
    );
}

#[test]
fn rejects_unreadable_actual_inside_projection_base_call() {
    let err = CapabilityHarness::new()
        .check(
            r#"
bundle Wrapped {
    bit: Bit,
}

map wrap(x: Bit) -> Wrapped =
    Wrapped {
        bit: x,
    }

module Bad(leak: out Bit, y: out Bit) {
    signal tmp: Bit := wrap(leak).bit
    y := tmp
}
"#,
        )
        .expect_err("projection base calls must still check their argument capabilities");

    assert!(err.contains("leak is not readable"));
}

#[test]
fn rejects_reading_out_port_after_local_drive() {
    let err = CapabilityHarness::new()
        .check(
            r#"
module Bad(y: out Bit, z: out Bit) {
    y := 1
    z := y
}
"#,
        )
        .expect_err("a local drive must not turn an out port into a readable source");

    assert!(err.contains("y is not readable"));
}

#[test]
fn rejects_unknown_read_source() {
    let err = CapabilityHarness::new()
        .check(
            r#"
module Bad(y: out Bit) {
    y := missing
}
"#,
        )
        .expect_err("unknown read roots must not be treated as readable values");

    assert!(err.contains("unresolved name missing"));
}

#[test]
fn allows_reading_same_indexed_local_view_field_after_drive() {
    CapabilityHarness::new()
        .check(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}

module Good(y: out Bit) {
    signal mid: [2] Stream<Bit>.sink
    mid[0].ready := 1
    mid[1].ready := 0
    y := mid[0].ready
}
"#,
        )
        .expect("exact indexed local drive readback should be allowed");
}

#[test]
fn rejects_reading_other_projection_after_local_projection_drive() {
    let err = CapabilityHarness::new()
        .check(
            r#"
module Bad(y: out [2] Bit, z: out Bit) {
    y[0] := 1
    z := y[1]
}
"#,
        )
        .expect_err("driving one projection must not make sibling projections readable");

    assert!(err.contains("y[1] is not readable"));
}

#[test]
fn local_drive_readback_uses_shadowed_local_identity() {
    let err = CapabilityHarness::new()
        .check(
            r#"
interface Stream<T> {
    payload: T
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}

module Bad<ENABLE: Bool>(y: out Bit) {
    signal mid: Stream<Bit>.sink
    if ENABLE {
        signal mid: Stream<Bit>.sink
        mid.ready := 1
    }
    y := mid.ready
}
"#,
        )
        .expect_err("a shadowed inner drive must not grant readback on the outer local");

    assert!(err.contains("mid.ready is not readable"));
}

#[test]
fn unknown_assignment_target_reports_unknown_identifier_with_target_span() {
    let source = r#"
module Bad(y: out Bit) {
    missing := 1
    y := 0
}
"#;
    let source_id = SourceId::new(10);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let output = MiddleCompiler::new().session(&[file]).check();
    let diagnostic = output
        .diagnostics()
        .first()
        .expect("unknown assignment target must be reported");
    let start = source
        .find("missing")
        .expect("test fixture must contain unknown target");

    assert_eq!(
        diagnostic.span,
        Span::new_in(source_id, start, start + "missing".len())
    );
}
