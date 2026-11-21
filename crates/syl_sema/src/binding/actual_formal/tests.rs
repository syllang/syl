use super::*;
use crate::{CompileError, EirError, LoweringError};

#[derive(Clone, Debug)]
struct TestFormal(&'static str);

impl FormalBinding for TestFormal {
    fn binding_name(&self) -> &str {
        self.0
    }
}

fn diagnostic_message(error: CompileError) -> String {
    error.to_string()
}

#[test]
fn resolves_mixed_named_and_positional_arguments_in_formal_order() {
    let formals = [TestFormal("a"), TestFormal("b"), TestFormal("c")];
    let mut binder = ActualFormalBinder::new(&formals);

    let first = binder
        .resolve("Child", Some("c"), Span::new(1, 2))
        .expect("named lookup must resolve by index");
    let second = binder
        .resolve("Child", None, Span::new(3, 4))
        .expect("positional lookup must skip used formals");
    let third = binder
        .resolve("Child", None, Span::new(5, 6))
        .expect("positional lookup must continue from the next unused formal");

    assert_eq!(first.binding_name(), "c");
    assert_eq!(second.binding_name(), "a");
    assert_eq!(third.binding_name(), "b");
}

#[test]
fn rejects_duplicate_named_arguments() {
    let formals = [TestFormal("a"), TestFormal("b")];
    let mut binder = ActualFormalBinder::new(&formals);

    binder
        .resolve("Child", Some("a"), Span::new(10, 11))
        .expect("first binding must succeed");
    let err = binder
        .resolve("Child", Some("a"), Span::new(12, 13))
        .expect_err("second binding to the same formal must fail");
    let diagnostic = err.to_diagnostic();

    assert!(matches!(
        err,
        CompileError::Lowering { kind, .. }
            if matches!(kind.as_ref(), LoweringError::Eir(EirError::DuplicateArgument { name }) if name == "a")
    ));
    assert_eq!(diagnostic.span, Span::new(12, 13));
    assert_eq!(
        diagnostic.code.as_deref(),
        Some("E_MIDDLE_DUPLICATE_ARGUMENT")
    );
}

#[test]
fn rejects_unknown_named_arguments_at_argument_span() {
    let formals = [TestFormal("a")];
    let mut binder = ActualFormalBinder::new(&formals);

    let err = binder
        .resolve("Child", Some("missing"), Span::new(30, 31))
        .expect_err("missing named arguments must fail");
    let diagnostic = err.to_diagnostic();

    assert!(matches!(
        err,
        CompileError::Lowering { kind, .. }
            if matches!(kind.as_ref(), LoweringError::Eir(EirError::UnknownParameter { name, callable }) if name == "missing" && callable == "Child")
    ));
    assert_eq!(diagnostic.span, Span::new(30, 31));
    assert_eq!(
        diagnostic.code.as_deref(),
        Some("E_MIDDLE_UNKNOWN_PARAMETER")
    );
}

#[test]
fn rejects_too_many_positional_arguments() {
    let formals = [TestFormal("a")];
    let mut binder = ActualFormalBinder::new(&formals);

    binder
        .resolve("Child", None, Span::new(20, 21))
        .expect("first positional argument must bind");
    let err = binder
        .resolve("Child", None, Span::new(22, 23))
        .expect_err("excess positional arguments must fail");
    let diagnostic = err.to_diagnostic();

    assert!(diagnostic_message(err).contains("too many positional arguments"));
    assert_eq!(diagnostic.span, Span::new(22, 23));
    assert_eq!(
        diagnostic.code.as_deref(),
        Some("E_MIDDLE_TOO_MANY_POSITIONAL_ARGS")
    );
}
