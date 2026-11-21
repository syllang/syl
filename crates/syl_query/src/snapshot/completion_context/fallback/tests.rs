use super::super::{CompletionContext, SourceItemContext};
use super::CompletionSourceAnalyzer;

#[test]
fn binding_equals_stays_expression_context() {
    assert_eq!(
        analyze(
            "module Top() {\n    let value = \n}\n",
            "let value = ",
            SourceItemContext::Callable
        ),
        Some(CompletionContext::Expression)
    );
}

#[test]
fn next_equals_is_rejected() {
    assert_eq!(
        analyze(
            "module Top() {\n    next state = value\n}\n",
            "state = value",
            SourceItemContext::Callable
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn next_equals_is_rejected_from_statement_start() {
    assert_eq!(
        analyze_from_cursor(
            "module Top() {\n    next state = value\n}\n",
            "state = value",
            SourceItemContext::Callable,
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn signal_equals_is_rejected() {
    assert_eq!(
        analyze(
            "module Top() {\n    signal ready: Bit = value\n}\n",
            "Bit = value",
            SourceItemContext::Callable
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn signal_equals_is_rejected_from_statement_start() {
    assert_eq!(
        analyze_from_cursor(
            "module Top() {\n    signal ready: Bit = value\n}\n",
            "ready: Bit = value",
            SourceItemContext::Callable,
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn let_colon_eq_is_rejected() {
    assert_eq!(
        analyze(
            "module Top() {\n    let value := input\n}\n",
            "value := input",
            SourceItemContext::Callable
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn let_colon_eq_is_rejected_from_statement_start() {
    assert_eq!(
        analyze_from_cursor(
            "module Top() {\n    let value := input\n}\n",
            "value := input",
            SourceItemContext::Callable,
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn drive_context_stays_enabled_in_callable_items() {
    assert_eq!(
        analyze(
            "module Top() {\n    out := \n}\n",
            "out := ",
            SourceItemContext::Callable
        ),
        Some(CompletionContext::Expression)
    );
}

#[test]
fn drive_context_is_rejected_in_functions() {
    assert_eq!(
        analyze(
            "fn update() {\n    value := next_value\n}\n",
            "value := next_value",
            SourceItemContext::Function
        ),
        Some(CompletionContext::Invalid)
    );
}

#[test]
fn software_assignment_stays_enabled_in_functions() {
    assert_eq!(
        analyze(
            "fn update() {\n    value = \n}\n",
            "value = ",
            SourceItemContext::Function
        ),
        Some(CompletionContext::Expression)
    );
}

fn analyze(
    source: &str,
    needle: &str,
    item_context: SourceItemContext,
) -> Option<CompletionContext> {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("fixture must contain marker {needle:?}"))
        + needle.len();
    CompletionSourceAnalyzer::new(source, offset, item_context).analyze()
}

fn analyze_from_cursor(
    source: &str,
    needle: &str,
    item_context: SourceItemContext,
) -> Option<CompletionContext> {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("fixture must contain marker {needle:?}"));
    CompletionSourceAnalyzer::new(source, offset, item_context).analyze()
}
