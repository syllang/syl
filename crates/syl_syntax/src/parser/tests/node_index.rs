use super::*;

#[test]
fn node_index_find_by_span_requires_matching_source_id() {
    let source = "const A = 1;\n";
    let output = SourceParser::new_in(source, SourceId::new(7)).parse_file_partial();
    let item = match &output.file.items[0] {
        Item::Const(item) => item,
        other => panic!("unexpected item: {other:?}"),
    };

    assert!(
        output.node_index().find_by_span(item.span).is_some(),
        "the exact span from the parsed file should resolve"
    );
    assert!(
        output
            .node_index()
            .find_by_span(Span::new_in(
                SourceId::new(8),
                item.span.start,
                item.span.end
            ))
            .is_none(),
        "changing only Span::source must prevent an exact match"
    );
}

#[test]
fn spans_cover_param_field_and_named_expr_prefixes() {
    let source = r#"
bundle Pair {
    left: Bit,
}

cell Top(x: in Bit, y: out Pair) {
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
        Item::Cell(item) => item,
        other => panic!("unexpected second item: {other:?}"),
    };

    let field = bundle.fields.first().expect("bundle field should exist");
    let param = module.params.first().expect("cell param should exist");
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
    assert!(
        output
            .node_index()
            .find_by_span(signal_field.span)
            .is_some()
    );
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
