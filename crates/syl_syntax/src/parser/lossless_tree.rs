use crate::{
    AstFile, LosslessNodeKind, LosslessSyntaxElement, LosslessSyntaxFile, LosslessSyntaxNode,
    LosslessToken,
};
use syl_span::{SourceId, Span};

/// Build a lossless syntax tree from the flat token list and AST items.
///
/// **Token distribution logic:** Tokens are assigned to items by span comparison:
/// 1. **Leading trivia** (tokens ending before the item starts) goes to the item
///    as trivia children, preserving whitespace and comments before declarations.
/// 2. **Interior tokens** (tokens starting before the item ends) are the item's
///    body tokens — keywords, identifiers, punctuation, etc.
/// 3. **Trailing tokens** (any tokens after the last item) become a `Trivia` node.
///
/// **Invariant:** Every token is placed into exactly one item or trailing trivia.
/// No tokens are dropped. This guarantees exact source reconstruction via
/// `LosslessSyntaxFile::write_source` / `source_text`.
///
/// **Empty source edge case:** If `file.items` is empty, all tokens become
/// trailing trivia under a single `Trivia` node. The file root always spans
/// `[0, source_len)`.
pub(super) fn build_lossless_syntax_file(
    source_id: SourceId,
    source_len: usize,
    file: &AstFile,
    tokens: Vec<LosslessToken>,
) -> LosslessSyntaxFile {
    let file_span = Span::new_in(source_id, 0, source_len);
    let mut tokens = tokens.into_iter().peekable();
    let mut flat_tokens = Vec::new();
    let mut children = Vec::new();

    for item in &file.items {
        let item_span = item.span();
        let item_kind = item.lossless_kind();
        let elements = collect_item_elements(&mut tokens, &mut flat_tokens, item_span);
        let span = node_span(elements.as_slice()).unwrap_or(item_span);
        children.push(LosslessSyntaxElement::Node(LosslessSyntaxNode::new(
            LosslessNodeKind::Item(item_kind),
            span,
            elements,
        )));
    }

    let trailing = collect_remaining_elements(&mut tokens, &mut flat_tokens);
    if !trailing.is_empty() {
        let span = node_span(trailing.as_slice()).unwrap_or(file_span);
        children.push(LosslessSyntaxElement::Node(LosslessSyntaxNode::new(
            LosslessNodeKind::Trivia,
            span,
            trailing,
        )));
    }

    let root = LosslessSyntaxNode::new(LosslessNodeKind::File, file_span, children);
    LosslessSyntaxFile::new(root, flat_tokens)
}

fn collect_item_elements(
    tokens: &mut std::iter::Peekable<std::vec::IntoIter<LosslessToken>>,
    flat_tokens: &mut Vec<LosslessToken>,
    item_span: Span,
) -> Vec<LosslessSyntaxElement> {
    let mut elements = Vec::new();

    while tokens
        .peek()
        .is_some_and(|token| token.span.end <= item_span.start)
    {
        push_token(
            &mut elements,
            flat_tokens,
            tokens.next().expect("peeked token must exist"),
        );
    }

    while tokens
        .peek()
        .is_some_and(|token| token.span.start < item_span.end)
    {
        push_token(
            &mut elements,
            flat_tokens,
            tokens.next().expect("peeked token must exist"),
        );
    }

    elements
}

fn collect_remaining_elements(
    tokens: &mut std::iter::Peekable<std::vec::IntoIter<LosslessToken>>,
    flat_tokens: &mut Vec<LosslessToken>,
) -> Vec<LosslessSyntaxElement> {
    let mut elements = Vec::new();
    for token in tokens.by_ref() {
        push_token(&mut elements, flat_tokens, token);
    }
    elements
}

fn push_token(
    elements: &mut Vec<LosslessSyntaxElement>,
    flat_tokens: &mut Vec<LosslessToken>,
    token: LosslessToken,
) {
    flat_tokens.push(token.clone());
    elements.push(LosslessSyntaxElement::Token(token));
}

fn node_span(elements: &[LosslessSyntaxElement]) -> Option<Span> {
    let first = elements.first()?;
    let mut span = first.span();
    for element in &elements[1..] {
        span = span.join(element.span());
    }
    Some(span)
}
