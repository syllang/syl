use crate::mapping::LspMapper;
use syl_session::TextDocumentEdit;
use tower_lsp::lsp_types::TextDocumentContentChangeEvent;

pub(crate) fn to_text_document_edits(
    mapper: &LspMapper,
    changes: Vec<TextDocumentContentChangeEvent>,
) -> Vec<TextDocumentEdit> {
    changes
        .into_iter()
        .map(|change| match change.range {
            Some(range) => TextDocumentEdit::incremental(mapper.source_range(range), change.text),
            None => TextDocumentEdit::full(change.text),
        })
        .collect()
}

#[cfg(test)]
mod tests;
