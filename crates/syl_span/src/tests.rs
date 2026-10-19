use super::*;

#[test]
fn empty_file_utf16_range_maps_to_origin() {
    let file = SourceFile::new(SourceId::new(7), "empty.syl", "");
    let span = Span::new_in(file.id(), 0, 0);

    let range = file
        .utf16_range(span)
        .expect("empty-file span with matching source must still resolve");

    assert_eq!(range.start, SourcePosition::new(0, 0));
    assert_eq!(range.end, SourcePosition::new(0, 0));
}

#[test]
fn empty_file_clamps_any_offset_to_origin() {
    let file = SourceFile::new(SourceId::new(8), "empty.syl", "");
    let span = Span::new_in(file.id(), 12, 34);

    let range = file
        .utf16_range(span)
        .expect("empty-file span with matching source must still resolve");

    assert_eq!(range.start, SourcePosition::new(0, 0));
    assert_eq!(range.end, SourcePosition::new(0, 0));
}
