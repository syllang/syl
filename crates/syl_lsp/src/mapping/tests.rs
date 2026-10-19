use super::*;

#[test]
fn hover_mapping_preserves_contents_and_optional_range() {
    let mapper = LspMapper::new();
    let hover = HoverResult::new(
        "generic T",
        Some(SourceRange::new(
            SourcePosition::new(3, 10),
            SourcePosition::new(3, 11),
        )),
    );

    let mapped = mapper.hover(hover);

    assert!(matches!(
        mapped.contents,
        HoverContents::Scalar(MarkedString::String(ref contents)) if contents == "generic T"
    ));
    let range = mapped.range.expect("hover range should be mapped");
    assert_eq!(range.start.line, 3);
    assert_eq!(range.start.character, 10);
}
