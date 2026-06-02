use syl_span::Span;

use crate::model::{HirBlock, HirCallable, HirCallableItem, HirDefKind, HirExternCellItem};

fn empty_block() -> HirBlock {
    HirBlock {
        stmts: vec![],
        tail: None,
        span: Span::default(),
    }
}

fn empty_callable_item() -> HirCallableItem {
    HirCallableItem {
        doc: None,
        name: String::new(),
        generics: vec![],
        params: vec![],
        ports: vec![],
        result: None,
        body: empty_block(),
        span: Span::default(),
    }
}

fn empty_extern_cell_item() -> HirExternCellItem {
    HirExternCellItem {
        doc: None,
        name: String::new(),
        generics: vec![],
        params: vec![],
        ports: vec![],
        result: None,
        span: Span::default(),
    }
}

#[test]
fn def_kind_summary_tags_are_stable_internal_offsets() {
    assert_eq!(HirDefKind::Const.summary_count(), 1);
    assert_eq!(HirDefKind::Fn.summary_count(), 2);
    assert_eq!(HirDefKind::Enum.summary_count(), 3);
    assert_eq!(HirDefKind::Struct.summary_count(), 4);
    assert_eq!(HirDefKind::Bundle.summary_count(), 5);
    assert_eq!(HirDefKind::Interface.summary_count(), 6);
    assert_eq!(HirDefKind::Map.summary_count(), 7);
    assert_eq!(HirDefKind::Cell.summary_count(), 8);
    assert_eq!(HirDefKind::ExternCell.summary_count(), 10);
}

#[test]
fn callable_summary_tags_are_stable_internal_offsets() {
    let cell = empty_callable_item();
    let extern_cell = empty_extern_cell_item();

    assert_eq!(cell.summary_count(), 0);
    assert_eq!(extern_cell.summary_count(), 0);
    assert_eq!(HirCallable::Cell(cell).summary_count(), 1);
    assert_eq!(HirCallable::Extern(extern_cell).summary_count(), 3);
}
