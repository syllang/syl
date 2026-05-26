use std::{
    fs,
    path::{Path, PathBuf},
};

use syl_span::SourceId;
use syl_syntax::{Item, SourceParser, Stmt};

#[test]
fn architecture_phase2_frontend_entry_stays_small_and_split() {
    let lib_path = workspace_root().join("crates/syl_syntax/src/lib.rs");
    let lib = read_text(&lib_path);
    let line_count = lib.lines().count();

    assert!(
        line_count <= 80,
        "syl_syntax lib.rs should stay thin after the Phase 2 split, got {line_count} lines"
    );
    for required in [
        "mod ast;",
        "mod node_index;",
        "pub mod token;",
        "pub mod lexer;",
        "pub mod parser;",
        "pub use node_index::{AstNodeId, AstNodeIndex, AstNodeKind, AstNodeRecord};",
    ] {
        assert!(
            lib.contains(required),
            "syl_syntax lib.rs must expose split frontend modules: missing {required:?}"
        );
    }
    for forbidden in [
        "pub struct AstFile",
        "pub enum Item",
        "pub enum TokenKind",
        "pub struct Parser",
        "fn recover_item_boundary",
        "fn recover_stmt_boundary",
    ] {
        assert!(
            !lib.contains(forbidden),
            "syl_syntax lib.rs must not inline frontend implementation details: {forbidden:?}"
        );
    }
}

#[test]
fn architecture_phase2_frontend_examples_parse_without_diagnostics() {
    let example_files = syl_files_under(&workspace_root().join("examples"));
    assert!(
        !example_files.is_empty(),
        "expected examples/**/*.syl fixtures for Phase 2 coverage"
    );

    for (idx, path) in example_files.iter().enumerate() {
        let source = read_text(path);
        let output = SourceParser::new_in(&source, SourceId::new(idx)).parse_file_partial();
        assert!(
            output.diagnostics.is_empty(),
            "{} should parse without frontend diagnostics:\n{}",
            path.display(),
            diagnostics_text(&output.diagnostics)
        );
        assert!(
            !output.node_index().is_empty(),
            "{} should produce a stable syntax node index",
            path.display()
        );
    }
}

#[test]
fn architecture_phase2_frontend_recovery_keeps_ast_usable() {
    let output = SourceParser::new(
        r#"
module Top(x: in Bit, y: out Bit) {
    signal broken: Bit := compile_error()
    y := x
}

module Tail(a: in Bit, b: out Bit) {
    b := a
}
"#,
    )
    .parse_file_partial();

    assert!(
        !output.diagnostics.is_empty(),
        "invalid syntax should emit syntax-owned diagnostics"
    );
    assert_eq!(output.file.items.len(), 2);

    match &output.file.items[0] {
        Item::Module(item) => {
            assert!(matches!(item.body.stmts.first(), Some(Stmt::Error { .. })));
            let recovered_drive = item
                .body
                .stmts
                .iter()
                .any(|stmt| matches!(stmt, Stmt::Drive { .. }));
            assert!(recovered_drive);
        }
        other => panic!("unexpected first item after recovery: {other:?}"),
    }
    match &output.file.items[1] {
        Item::Module(item) => assert_eq!(item.name, "Tail"),
        other => panic!("unexpected second item after recovery: {other:?}"),
    }
}

#[test]
fn architecture_phase2_frontend_node_ids_stay_stable_and_ranges_precise() {
    let base = "const A = 1;\nconst B = A;\n";
    let with_comment = "const A = 1;\n// retained trivia\nconst B = A;\n";

    let base_output = SourceParser::new(base).parse_file_partial();
    let commented_output = SourceParser::new(with_comment).parse_file_partial();

    let base_item = base_output
        .file
        .items
        .get(1)
        .expect("base source should contain a second item");
    let commented_item = commented_output
        .file
        .items
        .get(1)
        .expect("commented source should contain a second item");

    let base_record = base_output
        .node_index()
        .find_by_span(base_item.span())
        .expect("base node index should track the second item");
    let commented_record = commented_output
        .node_index()
        .find_by_span(commented_item.span())
        .expect("commented node index should track the second item");

    assert_eq!(base_record.id(), commented_record.id());
    assert_eq!(base_record.range().start.line, 1);
    assert_eq!(base_record.range().start.character, 0);
    assert_eq!(base_record.range().end.line, 1);
    assert_eq!(base_record.range().end.character, "const B = A;".len());
    assert_eq!(commented_record.range().start.line, 2);
    assert_eq!(commented_record.range().start.character, 0);
}

#[test]
fn architecture_phase2_frontend_node_ids_survive_same_text_sibling_insertions() {
    let base = "const A = 1;\nconst A = 1;\nconst B = A;\n";
    let expanded = "const A = 1;\nconst A = 1;\nconst A = 1;\nconst B = A;\n";

    let base_output = SourceParser::new(base).parse_file_partial();
    let expanded_output = SourceParser::new(expanded).parse_file_partial();

    let base_record = base_output
        .node_index()
        .find_by_span(
            base_output
                .file
                .items
                .get(2)
                .expect("base source should contain const B")
                .span(),
        )
        .expect("base node index should track const B");
    let expanded_record = expanded_output
        .node_index()
        .find_by_span(
            expanded_output
                .file
                .items
                .get(3)
                .expect("expanded source should contain const B")
                .span(),
        )
        .expect("expanded node index should track const B");

    assert_eq!(base_record.id(), expanded_record.id());
}

#[test]
fn architecture_phase2_frontend_ast_dump_stays_stable() {
    let source = concat!(
        "const WIDTH = 8;\n",
        "fn id(x: UInt<8>) -> UInt<8> { return x; }\n",
        "bundle Pair {\n",
        "    left: Bit\n",
        "}\n",
        "interface Stream<T> {\n",
        "    payload: T\n",
        "    valid: Bit\n",
        "\n",
        "    view source {\n",
        "        out payload\n",
        "        out valid\n",
        "    }\n",
        "}\n",
        "map keep(x: Bit) -> Bit =\n",
        "    x\n",
        "module Top(x: in Bit, y: out Bit) {\n",
        "    y := x\n",
        "}\n",
    );
    let file = SourceParser::new(source)
        .parse_file()
        .unwrap_or_else(|errors| {
            panic!(
                "frontend golden source must parse:\n{}",
                diagnostics_text(&errors)
            )
        });

    let expected = format!(
        "ast items=6 [const WIDTH@{}..{}, fn id@{}..{}, bundle Pair@{}..{}, interface Stream@{}..{}, map keep@{}..{}, module Top@{}..{}]",
        span_of(source, "const WIDTH = 8;").0,
        span_of(source, "const WIDTH = 8;").1,
        span_of(source, "fn id(x: UInt<8>) -> UInt<8> { return x; }").0,
        span_of(source, "fn id(x: UInt<8>) -> UInt<8> { return x; }").1,
        span_of(source, "bundle Pair {\n    left: Bit\n}").0,
        span_of(source, "bundle Pair {\n    left: Bit\n}").1,
        span_of(
            source,
            "interface Stream<T> {\n    payload: T\n    valid: Bit\n\n    view source {\n        out payload\n        out valid\n    }\n}",
        )
        .0,
        span_of(
            source,
            "interface Stream<T> {\n    payload: T\n    valid: Bit\n\n    view source {\n        out payload\n        out valid\n    }\n}",
        )
        .1,
        span_of(source, "map keep(x: Bit) -> Bit =\n    x").0,
        span_of(source, "map keep(x: Bit) -> Bit =\n    x").1,
        span_of(
            source,
            "module Top(x: in Bit, y: out Bit) {\n    y := x\n}",
        )
        .0,
        span_of(
            source,
            "module Top(x: in Bit, y: out Bit) {\n    y := x\n}",
        )
        .1,
    );

    assert_eq!(file.debug_dump(), expected);
}

fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|path| path.parent())
        .expect("sylc crate should live under workspace/crates")
        .to_path_buf()
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()))
}

fn span_of(source: &str, snippet: &str) -> (usize, usize) {
    let start = source
        .find(snippet)
        .unwrap_or_else(|| panic!("golden snippet should exist:\n{snippet}"));
    (start, start + snippet.len())
}

fn diagnostics_text(diagnostics: &[syl_span::Diagnostic]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}

fn syl_files_under(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_syl_files(root, &mut files);
    files.sort();
    files
}

fn collect_syl_files(dir: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir)
        .unwrap_or_else(|error| panic!("failed to read directory {}: {error}", dir.display()));
    for entry in entries {
        let entry = entry
            .unwrap_or_else(|error| panic!("failed to read entry in {}: {error}", dir.display()));
        let path = entry.path();
        if path.is_dir() {
            collect_syl_files(&path, files);
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("syl") {
            files.push(path);
        }
    }
}
