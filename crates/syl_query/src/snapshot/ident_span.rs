use crate::LocationResult;
use syl_hir::{HirDef, HirLocal};
use syl_session::{AnalysisSnapshot, DocumentUri};
use syl_span::{SourceMap, SourcePosition, Span};
use syl_syntax::{
    lexer::Lexer,
    token::{Token, TokenKind},
};

/// Lex a source slice while retaining absolute byte coordinates.
pub(super) fn tokens(map: &SourceMap, span: Span) -> Vec<Token> {
    let Some(text) = map
        .file(span.source)
        .and_then(|f| f.text().get(span.start..span.end))
    else {
        return Vec::new();
    };
    Lexer::new_in(text, span.source)
        .lex_all_partial()
        .tokens
        .into_iter()
        .map(|mut token| {
            token.span.start += span.start;
            token.span.end += span.start;
            token
        })
        .collect()
}

pub(super) fn name_span(map: &SourceMap, span: Span, name: &str, last: bool) -> Option<Span> {
    let mut matches = tokens(map, span).into_iter().filter_map(|token| {
        matches!(&token.kind, TokenKind::Ident(value) if value == name).then_some(token.span)
    });
    if last {
        matches.next_back()
    } else {
        matches.next()
    }
}

pub(super) fn def_name_span(map: &SourceMap, def: &HirDef) -> Option<Span> {
    name_span(map, def.span, &def.name, false)
}

pub(super) fn local_name_span(map: &SourceMap, local: &HirLocal) -> Option<Span> {
    name_span(map, local.span, &local.name, false)
}

pub(super) fn location(snapshot: &AnalysisSnapshot, span: Span) -> Option<LocationResult> {
    Some(LocationResult {
        uri: DocumentUri::new(snapshot.source_map().file(span.source)?.uri()),
        range: snapshot.source_map().utf16_range(span)?,
    })
}

pub(super) fn cursor(
    snapshot: &AnalysisSnapshot,
    uri: &DocumentUri,
    position: SourcePosition,
) -> Option<Span> {
    let file = snapshot.file_by_uri(uri)?;
    let source = snapshot.source_map().file(file.source_id())?;
    let offset = source.byte_offset_for_utf16_position(position);
    Some(Span::new_in(file.source_id(), offset, offset))
}

pub(super) fn contains(outer: Span, inner: Span) -> bool {
    outer.source == inner.source && outer.start <= inner.start && inner.end <= outer.end
}

pub(super) fn range_key(range: syl_span::SourceRange) -> (usize, usize, usize, usize) {
    (
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
    )
}
