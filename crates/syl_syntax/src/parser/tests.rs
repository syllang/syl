use super::*;

fn t(kind: TokenKind, start: usize, end: usize) -> Token {
    Token::new(kind, Span::new(start, end))
}

#[test]
fn parses_pratt_expression() {
    let mut parser = Parser::new(vec![
        t(TokenKind::Ident("a".into()), 0, 1),
        t(TokenKind::Plus, 2, 3),
        t(TokenKind::Ident("b".into()), 4, 5),
        t(TokenKind::Star, 6, 7),
        t(TokenKind::Ident("c".into()), 8, 9),
    ]);
    let expr = parser.parse_expr(0).unwrap();
    match expr {
        Expr::Binary {
            op: BinaryOp::Add, ..
        } => {}
        other => panic!("unexpected expr: {other:?}"),
    }
}

#[test]
fn parses_const_and_fn_items() {
    let parser = Parser::new(vec![
        t(TokenKind::KwConst, 0, 5),
        t(TokenKind::Ident("X".into()), 6, 7),
        t(TokenKind::Eq, 8, 9),
        t(TokenKind::Int(1), 10, 11),
        t(TokenKind::Semi, 11, 12),
        t(TokenKind::KwFn, 13, 15),
        t(TokenKind::Ident("f".into()), 16, 17),
        t(TokenKind::LParen, 17, 18),
        t(TokenKind::Ident("x".into()), 18, 19),
        t(TokenKind::Colon, 19, 20),
        t(TokenKind::Ident("Nat".into()), 21, 24),
        t(TokenKind::RParen, 24, 25),
        t(TokenKind::LBrace, 26, 27),
        t(TokenKind::KwReturn, 28, 34),
        t(TokenKind::Ident("x".into()), 35, 36),
        t(TokenKind::Semi, 36, 37),
        t(TokenKind::RBrace, 38, 39),
    ]);
    let file = parser.parse_file().unwrap();
    assert_eq!(file.items.len(), 2);
}

#[test]
fn parses_source_from_lexer() {
    let file = SourceParser::new("const X = 1 + 2 * 3;")
        .parse_file()
        .unwrap();
    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Const(item) => match item.value {
            Expr::Binary {
                op: BinaryOp::Add, ..
            } => {}
            ref other => panic!("unexpected const value: {other:?}"),
        },
        other => panic!("unexpected item: {other:?}"),
    }
}

#[test]
fn parses_inout_ports_and_view_fields() {
    let source = r#"
interface Pad {
    data: Bit
    view pad {
        inout data
    }
}

extern module PadCell(
    pad: inout Pad.pad,
)
"#;
    let file = SourceParser::new(source).parse_file().unwrap();

    match &file.items[0] {
        Item::Interface(item) => {
            assert_eq!(item.views[0].fields[0].dir, ViewDirection::InOut);
        }
        other => panic!("unexpected interface item: {other:?}"),
    }
    match &file.items[1] {
        Item::ExternModule(item) => {
            assert_eq!(item.ports[0].dir, ParamDirection::InOut);
            assert_eq!(item.ports[0].drive, DriveCapability::ReadWrite);
        }
        other => panic!("unexpected extern module item: {other:?}"),
    }
}

#[test]
fn parses_this_receiver_on_map_and_fn() {
    let source = r#"
map fire<T>(this stage: Stage<T>.tap) -> Bit =
    stage.valid and stage.ready

fn width(this word: Word) -> Nat {
    return 1
}
"#;
    let file = SourceParser::new(source).parse_file().unwrap();

    match &file.items[0] {
        Item::Map(item) => {
            assert_eq!(item.name, "fire");
            assert!(item.params[0].is_receiver());
        }
        other => panic!("unexpected map item: {other:?}"),
    }
    match &file.items[1] {
        Item::Fn(item) => {
            assert_eq!(item.name, "width");
            assert!(item.params[0].is_receiver());
        }
        other => panic!("unexpected fn item: {other:?}"),
    }
}

#[test]
fn rejects_this_receiver_on_cell_port() {
    let errors = SourceParser::new("cell Bad(this x: Bit) {}\n")
        .parse_file()
        .expect_err("cell ports cannot be receivers");

    assert!(errors
        .iter()
        .any(|error| error.message == "module and cell ports cannot use `this` receiver"));
}

#[test]
fn rejects_non_leading_this_receiver() {
    let errors = SourceParser::new("map bad(x: Bit, this y: Bit) -> Bit = y\n")
        .parse_file()
        .expect_err("receiver must be first");

    assert!(errors
        .iter()
        .any(|error| error.message == "`this` receiver must be the first parameter"));
}

#[test]
fn rejects_directed_this_receiver() {
    let errors = SourceParser::new("map bad(this x: in Bit) -> Bit = x\n")
        .parse_file()
        .expect_err("receiver cannot have a port direction");

    assert!(errors
        .iter()
        .any(|error| error.message == "`this` receiver cannot have an in/out direction"));
}

#[test]
fn path_segments_accept_contextual_keywords() {
    let file = SourceParser::new("use std.bundle.ReadyValidWord\n")
        .parse_file()
        .unwrap();

    assert_eq!(file.items.len(), 1);
    match &file.items[0] {
        Item::Use(item) => assert_eq!(item.path, ["std", "bundle", "ReadyValidWord"]),
        other => panic!("unexpected item: {other:?}"),
    }
}

#[test]
fn package_declaration_is_rejected_as_top_level_syntax() {
    let errors = SourceParser::new("package std.bundle\n")
        .parse_file()
        .expect_err("package declarations are no longer syntax");

    assert!(errors.iter().any(|error| error.message == "expected item"));
}

#[test]
fn parses_typed_ast_with_comments_and_trivia() {
    let source = "const X = 1; // retained trivia\nconst Y = X;";
    let file = SourceParser::new(source).parse_file().unwrap();

    assert_eq!(file.items.len(), 2);
    match &file.items[0] {
        Item::Const(item) => assert_eq!(item.name, "X"),
        other => panic!("unexpected first item: {other:?}"),
    }
    match &file.items[1] {
        Item::Const(item) => assert_eq!(item.name, "Y"),
        other => panic!("unexpected second item: {other:?}"),
    }
}

#[test]
fn parse_file_with_lossless_preserves_trivia_order_spans_and_text() {
    let source = "const X = 1; // retained trivia\nconst Y = X;";
    let (output, syntax) = SourceParser::new(source).parse_file_with_lossless();

    assert!(output.diagnostics.is_empty());
    assert_eq!(output.file.items.len(), 2);

    let comment_index = syntax
        .tokens()
        .iter()
        .position(|token| matches!(token.kind, LosslessTokenKind::LineComment))
        .expect("line comment should be present in lossless syntax");
    let comment = &syntax.tokens()[comment_index];
    assert_eq!(comment.text.as_ref(), "// retained trivia");
    assert_eq!(comment.span.start, source.find("//").unwrap());
    assert_eq!(comment.span.end, comment.span.start + comment.text.len());

    let after_comment = &syntax.tokens()[comment_index + 1];
    assert!(matches!(after_comment.kind, LosslessTokenKind::Whitespace));
    assert_eq!(after_comment.text.as_ref(), "\n");

    assert!(matches!(
        syntax.tokens()[0].kind,
        LosslessTokenKind::Keyword
    ));
    assert!(matches!(syntax.tokens()[2].kind, LosslessTokenKind::Ident));

    let reconstructed: String = syntax
        .tokens()
        .iter()
        .map(|token| token.text.as_ref())
        .collect();
    assert_eq!(reconstructed, source);
}

#[test]
fn parse_file_with_lossless_builds_item_nodes() {
    let source = "// lead\nconst A = 1;// between\n// next lead\nconst B = 2;// tail\n";
    let (_, syntax) = SourceParser::new(source).parse_file_with_lossless();

    let root = syntax.root();
    assert!(matches!(root.kind(), LosslessNodeKind::File));

    let item_nodes: Vec<_> = root
        .children()
        .iter()
        .filter_map(|element| match element {
            LosslessSyntaxElement::Node(node)
                if matches!(node.kind(), LosslessNodeKind::Item(_)) =>
            {
                Some(node)
            }
            _ => None,
        })
        .collect();

    assert_eq!(item_nodes.len(), 2);
    assert!(matches!(
        item_nodes[0].kind(),
        LosslessNodeKind::Item(LosslessItemKind::Const)
    ));
    assert!(matches!(
        item_nodes[1].kind(),
        LosslessNodeKind::Item(LosslessItemKind::Const)
    ));
    assert_eq!(item_nodes[0].span().start, 0);
    assert_eq!(
        item_nodes[1].span().start,
        source.find("// between").unwrap()
    );

    assert!(matches!(
        item_nodes[0].children().first(),
        Some(LosslessSyntaxElement::Token(token))
            if matches!(token.kind, LosslessTokenKind::LineComment)
    ));
    assert!(matches!(
        item_nodes[1].children().first(),
        Some(LosslessSyntaxElement::Token(token))
            if matches!(token.kind, LosslessTokenKind::LineComment)
    ));

    let trailing = root
        .children()
        .last()
        .expect("file should keep trailing trivia under the root");
    assert!(matches!(
        trailing,
        LosslessSyntaxElement::Node(node)
            if matches!(node.kind(), LosslessNodeKind::Trivia)
    ));
    assert!(matches!(
        trailing,
        LosslessSyntaxElement::Node(node)
            if matches!(
                node.children().first(),
                Some(LosslessSyntaxElement::Token(token))
                    if token.text.as_ref() == "// tail"
            )
    ));
}

#[test]
fn lossless_source_text_reconstructs_original_source() {
    let source = "\t// lead\nconst X = 1; // keep\n\nconst Y = X;\n";
    let (_, syntax) = SourceParser::new(source).parse_file_with_lossless();

    let mut reconstructed = String::new();
    syntax.write_source(&mut reconstructed);
    assert_eq!(reconstructed, source);
    assert_eq!(syntax.source_text(), source);
}

#[test]
fn parse_file_with_lossless_separates_keywords_from_identifiers() {
    let source = "const value = 1;";
    let (_, syntax) = SourceParser::new(source).parse_file_with_lossless();

    let kinds: Vec<_> = syntax
        .tokens()
        .iter()
        .filter_map(|token| match token.kind {
            LosslessTokenKind::Keyword => Some("keyword"),
            LosslessTokenKind::Ident => Some("ident"),
            LosslessTokenKind::Int => Some("int"),
            LosslessTokenKind::Bool => Some("bool"),
            LosslessTokenKind::Str => Some("str"),
            LosslessTokenKind::Punctuation => Some("punct"),
            LosslessTokenKind::Whitespace
            | LosslessTokenKind::LineComment
            | LosslessTokenKind::Unknown => None,
        })
        .collect();

    assert_eq!(kinds, ["keyword", "ident", "punct", "int", "punct"]);
}

#[test]
fn partial_parse_recovers_next_top_level_item() {
    let output = SourceParser::new(
        r#"
const A = ;
const B = 1;
"#,
    )
    .parse_file_partial();

    assert_eq!(output.file.items.len(), 2);
    assert!(!output.diagnostics.is_empty());
    match &output.file.items[0] {
        Item::Error(item) => {
            assert_eq!(item.span.start, 1);
            assert!(item.span.end > item.span.start);
        }
        other => panic!("unexpected error item: {other:?}"),
    }
    match &output.file.items[1] {
        Item::Const(item) => assert_eq!(item.name, "B"),
        other => panic!("unexpected recovered item: {other:?}"),
    }
}

#[test]
fn eof_diagnostics_keep_the_source_id() {
    let source = "module Top(x: in Bit) {";
    let source_id = SourceId::new(7);
    let output = SourceParser::new_in(source, source_id).parse_file_partial();
    let diagnostic = output
        .diagnostics
        .first()
        .expect("incomplete source should produce a diagnostic");

    assert_eq!(diagnostic.span.source, source_id);
    assert_eq!(diagnostic.span.start, source.len());
    assert_eq!(diagnostic.span.end, source.len());
}

#[test]
fn partial_parse_recovers_after_invalid_stmt_inside_block() {
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

    assert!(!output.diagnostics.is_empty());
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
        other => panic!("unexpected first item: {other:?}"),
    }

    match &output.file.items[1] {
        Item::Module(item) => assert_eq!(item.name, "Tail"),
        other => panic!("unexpected recovered item: {other:?}"),
    }
}

#[test]
fn assignments_are_contextual_statements() {
    let source = r#"
fn update(x: Bit) -> Bit {
    var y: Bit = x
    y = x
    y
}

module Top(x: in Bit, y: out Bit) {
    signal ready: Bit := x
    y := ready
    next state := x
}
"#;
    let file = SourceParser::new(source).parse_file().unwrap();

    match &file.items[0] {
        Item::Fn(item) => {
            assert!(matches!(item.body.stmts[1], Stmt::Assign { .. }));
            assert!(matches!(item.body.tail.as_deref(), Some(Expr::Ident(_, _))));
        }
        other => panic!("unexpected fn item: {other:?}"),
    }
    match &file.items[1] {
        Item::Module(item) => {
            assert!(matches!(item.body.stmts[0], Stmt::Signal { .. }));
            assert!(matches!(item.body.stmts[1], Stmt::Drive { .. }));
            assert!(matches!(item.body.stmts[2], Stmt::Next { .. }));
        }
        other => panic!("unexpected module item: {other:?}"),
    }
}

#[test]
fn rejects_mixed_assignment_operators_in_parser() {
    let output = SourceParser::new(
        r#"
fn bad() {
    let a := 0
    var b := 0
    b := a
}

module Top(x: in Bit, y: out Bit) {
    signal ready: Bit = x
    next state = x
    y = x
}
"#,
    )
    .parse_file_partial();

    let messages: Vec<_> = output
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect();

    assert!(messages.contains(&"`let` statements only accept `=`"));
    assert!(messages.contains(&"`var` statements only accept `=`"));
    assert!(messages.contains(&"`fn` blocks use `=`; `:=` is only valid in hardware blocks"));
    assert!(messages.contains(&"`signal` statements only accept `:=`"));
    assert!(messages.contains(&"`next` statements only accept `:=`"));
    assert!(messages.contains(&"hardware blocks use `:=`; bare `=` assignment is invalid here"));
}

#[test]
fn spans_cover_param_field_and_named_expr_prefixes() {
    let source = r#"
bundle Pair {
    left: Bit,
}

module Top(x: in Bit, y: out Pair) {
    signal pair: Pair := Pair {
        left: x,
    }
    y := pair
}
"#;

    let output = SourceParser::new(source).parse_file_partial();
    assert!(
        output.diagnostics.is_empty(),
        "expected clean parse, got diagnostics: {:?}",
        output.diagnostics
    );

    let bundle = match &output.file.items[0] {
        Item::Bundle(item) => item,
        other => panic!("unexpected first item: {other:?}"),
    };
    let module = match &output.file.items[1] {
        Item::Module(item) => item,
        other => panic!("unexpected second item: {other:?}"),
    };

    let field = bundle.fields.first().expect("bundle field should exist");
    let param = module.params.first().expect("module param should exist");
    let signal_field = match module.body.stmts.first() {
        Some(Stmt::Signal {
            value: Some(Expr::Aggregate { fields, .. }),
            ..
        }) => fields.first().expect("aggregate field should exist"),
        other => panic!("unexpected first stmt: {other:?}"),
    };

    let field_start = source
        .find("left: Bit")
        .expect("bundle field text should exist");
    let param_start = source.find("x: in Bit").expect("param text should exist");
    let aggregate_field_start = source
        .rfind("left: x")
        .expect("aggregate field text should exist");

    assert_eq!(field.span.start, field_start);
    assert_eq!(field.span.end, field_start + "left: Bit".len());
    assert_eq!(param.span.start, param_start);
    assert_eq!(param.span.end, param_start + "x: in Bit".len());
    assert_eq!(signal_field.span.start, aggregate_field_start);
    assert_eq!(
        signal_field.span.end,
        aggregate_field_start + "left: x".len()
    );

    assert!(output.node_index().find_by_span(field.span).is_some());
    assert!(output.node_index().find_by_span(param.span).is_some());
    assert!(output
        .node_index()
        .find_by_span(signal_field.span)
        .is_some());
}

#[test]
fn node_index_ids_stay_stable_when_leading_trivia_changes() {
    let base = "const A = 1;\nconst B = A;\n";
    let with_comment = "const A = 1;\n// retained trivia\nconst B = A;\n";

    let base_output = SourceParser::new(base).parse_file_partial();
    let commented_output = SourceParser::new(with_comment).parse_file_partial();

    let base_item = base_output
        .file
        .items
        .get(1)
        .expect("second item should exist in base source");
    let commented_item = commented_output
        .file
        .items
        .get(1)
        .expect("second item should exist in commented source");

    let base_record = base_output
        .node_index()
        .find_by_span(base_item.span())
        .expect("base node index should track second item");
    let commented_record = commented_output
        .node_index()
        .find_by_span(commented_item.span())
        .expect("commented node index should track second item");

    assert_eq!(base_record.id(), commented_record.id());
    assert_ne!(base_record.range(), commented_record.range());
}

#[test]
fn node_index_ids_stay_stable_when_preceding_identical_sibling_is_inserted() {
    let base = "const A = 1;\nconst A = 1;\nconst B = A;\n";
    let expanded = "const A = 1;\nconst A = 1;\nconst A = 1;\nconst B = A;\n";

    let base_output = SourceParser::new(base).parse_file_partial();
    let expanded_output = SourceParser::new(expanded).parse_file_partial();

    let base_b = base_output
        .file
        .items
        .get(2)
        .expect("base source should contain const B");
    let expanded_b = expanded_output
        .file
        .items
        .get(3)
        .expect("expanded source should contain const B");

    let base_b_record = base_output
        .node_index()
        .find_by_span(base_b.span())
        .expect("base node index should track const B");
    let expanded_b_record = expanded_output
        .node_index()
        .find_by_span(expanded_b.span())
        .expect("expanded node index should track const B");

    assert_eq!(base_b_record.id(), expanded_b_record.id());

    let base_a_ids: Vec<_> = base_output
        .file
        .items
        .iter()
        .take(2)
        .map(|item| {
            base_output
                .node_index()
                .find_by_span(item.span())
                .expect("base duplicate const should exist in the node index")
                .id()
        })
        .collect();
    let expanded_a_ids: Vec<_> = expanded_output
        .file
        .items
        .iter()
        .take(3)
        .map(|item| {
            expanded_output
                .node_index()
                .find_by_span(item.span())
                .expect("expanded duplicate const should exist in the node index")
                .id()
        })
        .collect();

    assert_eq!(base_a_ids.len(), 2);
    assert_ne!(base_a_ids[0], base_a_ids[1]);
    assert_eq!(expanded_a_ids.len(), 3);
    assert_ne!(expanded_a_ids[0], expanded_a_ids[1]);
    assert_ne!(expanded_a_ids[1], expanded_a_ids[2]);
    assert_eq!(base_a_ids[0], expanded_a_ids[0]);
    assert_eq!(base_a_ids[1], expanded_a_ids[1]);
}
