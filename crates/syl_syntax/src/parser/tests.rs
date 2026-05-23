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
