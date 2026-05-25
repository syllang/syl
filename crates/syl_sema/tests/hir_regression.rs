mod support;

use support::MiddleCompiler;
use syl_span::{SourceId, Span};
use syl_syntax::SourceParser;

struct TestCompiler {
    middle: MiddleCompiler,
}

impl TestCompiler {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
        }
    }

    fn check(&self, source: &str) -> Result<(), String> {
        let file = SourceParser::new(source).parse_file().map_err(|errs| {
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        self.middle
            .compile_files(&[file])
            .map(|_| ())
            .map_err(|err| err.to_string())
    }
}

#[test]
fn local_binding_shadows_same_named_global_generator() {
    let err = TestCompiler::new()
        .check(
            r#"
cell MakeBit() -> y: Bit {
    y := 1
}

module Top(MakeBit: in Bit, y: out Bit) {
    signal tmp: Bit := MakeBit()
    y := tmp
}
"#,
        )
        .expect_err("callee resolution must use LocalId/DefId, not a global string lookup");

    assert!(err.contains("hardware value expressions cannot call unknown function MakeBit"));
    assert!(!err.contains("cannot call generator MakeBit"));
}

#[test]
fn staged_middle_api_serves_definition_and_tir_hover() {
    let source = r#"
module Top(x: in Bit, y: out Bit) {
    y := x
}
"#;
    let source_id = SourceId::new(42);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let files = vec![file];
    let middle = MiddleCompiler::new();
    let session = middle.session(&files);
    let hir = session.resolve_hir().expect("HIR stage must resolve");
    let tir = hir.check_tir().expect("TIR stage must check");
    let x_offset = source.rfind('x').expect("test fixture must contain rhs x");
    let span = Span::new_in(source_id, x_offset, x_offset + 1);

    let definition = hir
        .definition_at(span)
        .expect("HIR definition lookup must resolve rhs x");
    let hover = tir
        .hover_at(span)
        .expect("TIR hover must expose phase and type for rhs x");

    assert_eq!(definition.name(), "x");
    assert!(hover.text().contains("Bit"));
    assert!(tir.type_count() > 0);
}

#[test]
fn staged_middle_tir_hover_infers_projection_types() {
    let source = r#"
bundle Word {
    lo: UInt<4>,
}

module Top(pkt: in Word, arr: in [2] Bit, y: out Bit) {
    y := pkt.lo[0]
    y := arr[0]
}
"#;
    let source_id = SourceId::new(43);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let files = vec![file];
    let middle = MiddleCompiler::new();
    let session = middle.session(&files);
    let hir = session.resolve_hir().expect("HIR stage must resolve");
    let tir = hir.check_tir().expect("TIR stage must check");
    let field_offset = source
        .find("pkt.lo")
        .expect("test fixture must contain field projection");
    let index_offset = source
        .rfind("arr[0]")
        .expect("test fixture must contain array index projection");
    let field_span = Span::new_in(source_id, field_offset, field_offset + "pkt.lo".len());
    let index_span = Span::new_in(source_id, index_offset, index_offset + "arr[0]".len());

    let field_hover = tir
        .hover_at(field_span)
        .expect("TIR hover must expose field projection type");
    let index_hover = tir
        .hover_at(index_span)
        .expect("TIR hover must expose array index type");

    assert!(field_hover.text().contains("UInt<4>"));
    assert!(index_hover.text().contains("Bit"));
}

#[test]
fn staged_middle_tir_hover_infers_generic_map_call_result() {
    let source = r#"
bundle Word<W: Nat> {
    lo: UInt<W>,
}

map low<W: Nat>(pkt: Word<W>) -> UInt<W> =
    pkt.lo

module Top(pkt: in Word<4>, y: out Bit) {
    y := low<4>(pkt)[0]
}
"#;
    let source_id = SourceId::new(44);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let files = vec![file];
    let middle = MiddleCompiler::new();
    let session = middle.session(&files);
    let hir = session.resolve_hir().expect("HIR stage must resolve");
    let tir = hir.check_tir().expect("TIR stage must check");
    let call_offset = source
        .find("low<4>(pkt)")
        .expect("test fixture must contain generic map call");
    let call_span = Span::new_in(source_id, call_offset, call_offset + "low<4>(pkt)".len());

    let hover = tir
        .hover_at(call_span)
        .expect("TIR hover must expose generic map return type");

    assert!(hover.text().contains("UInt<4>"));
}

#[test]
fn unimported_module_item_is_not_resolved_by_unique_short_name() {
    let lib = SourceParser::new(
        r#"
cell MakeBit() -> y: Bit {
    y := 1
}
"#,
    )
    .parse_file()
    .expect("library source must parse");
    let top = SourceParser::new(
        r#"
module Top(y: out Bit) {
    let tmp = place MakeBit()
    y := tmp
}
"#,
    )
    .parse_file()
    .expect("top source must parse");

    let result = MiddleCompiler::new().compile_files(&[lib, top]);
    let err = match result {
        Ok(_) => {
            panic!("unimported package item must not resolve through global short-name fallback")
        }
        Err(err) => err.to_string(),
    };

    assert!(
        err.contains("unresolved name MakeBit")
            || err.contains("unknown function MakeBit")
            || err.contains("unknown elaboration")
    );
    assert!(!err.contains("cannot call generator MakeBit"));
}

#[test]
fn unresolved_expr_name_is_diagnosed_in_hir() {
    let err = TestCompiler::new()
        .check(
            r#"
module Top(y: out Bit) {
    y := missing
}
"#,
        )
        .expect_err("unknown expression name must fail during HIR resolution");

    assert!(err.contains("unresolved name missing"), "{err}");
}

#[test]
fn diagnostics_collects_multiple_unresolved_expr_names_in_hir() {
    let source = r#"
module Top(y: out Bit) {
    y := first_missing
    y := second_missing
}
"#;
    let source_id = SourceId::new(12);
    let file = SourceParser::new_in(source, source_id)
        .parse_file()
        .expect("test source must parse");
    let diagnostics = MiddleCompiler::new().session(&[file]).diagnostics();
    let first_start = source
        .find("first_missing")
        .expect("test fixture must contain first unresolved name");
    let second_start = source
        .find("second_missing")
        .expect("test fixture must contain second unresolved name");

    assert_eq!(diagnostics.len(), 2);
    assert_eq!(
        diagnostics[0].span,
        Span::new_in(source_id, first_start, first_start + "first_missing".len())
    );
    assert_eq!(
        diagnostics[1].span,
        Span::new_in(
            source_id,
            second_start,
            second_start + "second_missing".len()
        )
    );
    assert!(
        diagnostics[0]
            .message
            .contains("unresolved name first_missing")
    );
    assert!(
        diagnostics[1]
            .message
            .contains("unresolved name second_missing")
    );
}

#[test]
fn check_collects_multiple_unresolved_expr_names_in_hir() {
    let source = r#"
module Top(y: out Bit) {
    y := first_missing
    y := second_missing
}
"#;
    let file = SourceParser::new(source)
        .parse_file()
        .expect("test source must parse");
    let output = MiddleCompiler::new().session(&[file]).check();
    let messages = output
        .diagnostics()
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(messages.len(), 2);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("first_missing"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("second_missing"))
    );
}

#[test]
fn diagnostics_collects_multiple_duplicate_items_in_hir_index() {
    let source = r#"
const WIDTH: Nat = 8
const WIDTH: Nat = 16
const DEPTH: Nat = 2
const DEPTH: Nat = 4

module Top(y: out Bit) {
    y := 0
}
"#;
    let file = SourceParser::new(source)
        .parse_file()
        .expect("test source must parse");
    let diagnostics = MiddleCompiler::new().session(&[file]).diagnostics();
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(messages.len(), 2);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate const WIDTH"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("duplicate const DEPTH"))
    );
}

#[test]
fn diagnostics_collects_multiple_unknown_imports_in_hir_index() {
    let source = r#"
use missing.First
use missing.Second

module Top(y: out Bit) {
    y := 0
}
"#;
    let file = SourceParser::new(source)
        .parse_file()
        .expect("test source must parse");
    let diagnostics = MiddleCompiler::new().session(&[file]).diagnostics();
    let messages = diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect::<Vec<_>>();

    assert_eq!(messages.len(), 2);
    assert!(
        messages
            .iter()
            .any(|message| message.contains("missing.First"))
    );
    assert!(
        messages
            .iter()
            .any(|message| message.contains("missing.Second"))
    );
}

#[test]
fn builtin_zero_call_is_not_reported_as_unresolved_name() {
    TestCompiler::new()
        .check(
            r#"
module Top(y: out Bit) {
    y := zero()
}
"#,
        )
        .expect("zero() is a builtin value expression, not a missing user symbol");
}

#[test]
fn select_default_pattern_is_not_reported_as_unresolved_name() {
    TestCompiler::new()
        .check(
            r#"
module Top(sel: in Bit, y: out Bit) {
    y := select priority {
        sel => 1,
        default => 0,
    }
}
"#,
        )
        .expect("select default is syntax, not a missing user symbol");
}

#[test]
fn unknown_drive_target_is_not_treated_as_writable() {
    let err = TestCompiler::new()
        .check(
            r#"
module Top(y: out Bit) {
    missing := 1
    y := 0
}
"#,
        )
        .expect_err("unknown assignment target must not receive implicit write capability");

    assert!(err.contains("missing"), "{err}");
}

#[test]
fn inline_cell_body_uses_cell_owner_for_map_resolution() {
    let lib = SourceParser::new(
        r#"
map passthrough(x: Bit) -> Bit =
    x

cell Make(x: in Bit) -> y: Bit {
    y := passthrough(x)
}
"#,
    )
    .parse_file()
    .expect("library source must parse");
    let top = SourceParser::new(
        r#"
use lib.Make

map passthrough(x: Bit) -> Bit =
    0

module Top(x: in Bit, y: out Bit) {
    let made = place Make(x: x)
    y := made
}
"#,
    )
    .parse_file()
    .expect("top source must parse");

    MiddleCompiler::new()
        .compile_files_with_paths(&[
            (vec!["lib".to_string()], lib),
            (vec!["top".to_string()], top),
        ])
        .expect("inlined cell body must resolve map calls through the cell owner DefId");
}

#[test]
fn ambiguous_same_leaf_imports_are_rejected() {
    let first = SourceParser::new(
        r#"
module Make(y: out Bit) {
    y := 1
}
"#,
    )
    .parse_file()
    .expect("first source must parse");
    let second = SourceParser::new(
        r#"
module Make(y: out Bit) {
    y := 0
}
"#,
    )
    .parse_file()
    .expect("second source must parse");
    let top = SourceParser::new(
        r#"
use first.Make
use second.Make

module Top(y: out Bit) {
    let u = place Make(y: y)
}
"#,
    )
    .parse_file()
    .expect("top source must parse");

    let result = MiddleCompiler::new().compile_files_with_paths(&[
        (vec!["first".to_string()], first),
        (vec!["second".to_string()], second),
        (vec!["top".to_string()], top),
    ]);
    let err = match result {
        Ok(_) => panic!("same leaf imports must be rejected before name lookup"),
        Err(err) => err.to_string(),
    };

    assert!(err.contains("ambiguous import Make"), "{err}");
    assert!(err.contains("first.Make"), "{err}");
    assert!(err.contains("second.Make"), "{err}");
}
