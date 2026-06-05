#![allow(dead_code)]
use syl_hir::{DefId, ExprId, HirDesign, LocalId};
use syl_span::{SourceId, Span};
use syl_syntax::{AstFile, SourceParser};

pub(crate) fn parse_sources(sources: &[&str]) -> Vec<AstFile> {
    sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            SourceParser::new_in(source, SourceId::new(index))
                .parse_file()
                .unwrap_or_else(|errors| {
                    panic!("fixture {index} must parse:\n{}", diagnostics_text(&errors))
                })
        })
        .collect()
}

pub(crate) fn def_id(hir: &HirDesign, name: &str) -> DefId {
    hir.defs
        .iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("missing definition {name}"))
        .id
}

pub(crate) fn def_id_by_path(hir: &HirDesign, path: &[&str]) -> DefId {
    hir.defs
        .iter()
        .find(|def| {
            def.canonical_path
                .segments()
                .iter()
                .map(String::as_str)
                .eq(path.iter().copied())
        })
        .unwrap_or_else(|| panic!("missing definition {}", path.join(".")))
        .id
}

pub(crate) fn local_id(hir: &HirDesign, owner: DefId, name: &str) -> LocalId {
    hir.locals
        .iter()
        .find(|local| local.owner == owner && local.name == name)
        .unwrap_or_else(|| panic!("missing local {name} in owner {}", owner.get()))
        .id
}

pub(crate) struct ExprLookup<'a> {
    source: &'a str,
    source_id: SourceId,
    needle: &'a str,
    start_offset: usize,
    width: usize,
}

impl<'a> ExprLookup<'a> {
    pub(crate) fn new(
        source: &'a str,
        source_id: SourceId,
        needle: &'a str,
        start_offset: usize,
        width: usize,
    ) -> Self {
        Self {
            source,
            source_id,
            needle,
            start_offset,
            width,
        }
    }
}

pub(crate) fn expr_id_at(lookup: ExprLookup<'_>, hir: &HirDesign) -> ExprId {
    let base = lookup
        .source
        .find(lookup.needle)
        .unwrap_or_else(|| panic!("missing needle {}", lookup.needle));
    let start = base + lookup.start_offset;
    let span = Span::new_in(lookup.source_id, start, start + lookup.width);
    hir.exprs
        .iter()
        .find(|expr| expr.span == span)
        .unwrap_or_else(|| panic!("missing expr at {}..{}", span.start, span.end))
        .id
}

pub(crate) fn diagnostics_text<T: ToString>(diagnostics: &[T]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
