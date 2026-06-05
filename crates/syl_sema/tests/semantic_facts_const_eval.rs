#[path = "semantic_facts/support.rs"]
mod semantic_facts_support;

use semantic_facts_support::{ExprLookup, def_id, expr_id_at};
use syl_sema::ir::const_mir::{ConstEvalEnv, ConstExprKind, ConstMirBuilder, ConstValue};
use syl_sema::{ConstEvalError, HirFactId, LoweringError, SemanticCompiler};
use syl_span::SourceId;
use syl_syntax::SourceParser;

#[test]
fn const_facts_are_deterministic_across_repeated_runs() {
    let source = r#"
fn add_one(x: nat) -> nat {
    return x + 1
}

const WIDTH: nat = add_one(4)
const HEIGHT: nat = add_one(4)

cell Top(y: out UInt<WIDTH>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("const determinism fixture must parse");
    let files = [file];
    let compiler = SemanticCompiler::new();

    let first = compiler.session(&files).check();
    let second = compiler.session(&files).check();
    let first_facts = first.facts().expect("first run must expose facts");
    let second_facts = second.facts().expect("second run must expose facts");
    let first_hir = first
        .tir()
        .expect("first run must produce TIR")
        .design()
        .hir();
    let width_def = def_id(first_hir, "WIDTH");
    let height_def = def_id(first_hir, "HEIGHT");
    let width_call = expr_id_at(
        ExprLookup::new(
            source,
            SourceId::new(0),
            "add_one(4)",
            0,
            "add_one(4)".len(),
        ),
        first_hir,
    );
    let add_one_body = expr_id_at(
        ExprLookup::new(source, SourceId::new(0), "x + 1", 0, "x + 1".len()),
        first_hir,
    );

    assert_eq!(first_facts.consts(), second_facts.consts());
    assert_eq!(
        first_facts.consts().value(HirFactId::Def(width_def)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        first_facts.consts().value(HirFactId::Def(height_def)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        first_facts.consts().value(HirFactId::Expr(width_call)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        first_facts.consts().value(HirFactId::Expr(add_one_body)),
        Some(ConstValue::Nat(5))
    );
}

#[test]
fn extension_fn_method_lowers_into_const_mir() {
    let source = r#"
enum Op {
    Add,
}

fn rank(this op: Op) -> nat {
    return 1
}

fn use_rank(op: Op) -> nat {
    return op.rank()
}

cell Top(y: out UInt<1>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("extension fn fixture must parse");
    let files = [file];
    let output = SemanticCompiler::new().session(&files).check();
    let tir = output
        .tir()
        .expect("extension fn fixture must type-check into TIR");
    let hir = tir.design().hir();
    let use_rank = def_id(hir, "use_rank");
    let program = ConstMirBuilder::new(tir.design())
        .build()
        .expect("extension fn call must lower into const MIR");
    let function = program
        .function(use_rank)
        .expect("use_rank const MIR function must exist");

    assert!(!function.is_unsupported());
}

#[test]
fn const_evaluator_reports_structured_step_limit_for_long_running_const_fn() {
    let source = r#"
fn burn_steps(limit: nat) -> nat {
    var i: nat = 0

    while i < limit {
        i = i + 1
    }

    return i
}

const WIDTH: nat = burn_steps(20000)

cell Top(y: out UInt<1>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("step-limit fixture must parse");
    let files = [file];
    let output = SemanticCompiler::new().session(&files).check();
    let tir = output
        .tir()
        .expect("step-limit fixture must still type-check into TIR");
    let hir = tir.design().hir();
    let width_def = def_id(hir, "WIDTH");
    let width_item = hir
        .consts
        .get(&width_def)
        .expect("WIDTH const item must exist");
    let program = ConstMirBuilder::new(tir.design())
        .build()
        .expect("const MIR program lowering must succeed");
    let expr = ConstMirBuilder::new(tir.design()).lower_const_expr(width_def, &width_item.value);
    let mut evaluator = program.evaluator();
    let err = evaluator
        .expr_value(&expr, &mut ConstEvalEnv::with_owner(Some(width_def)))
        .expect_err("long-running const fn must hit the evaluator step limit");

    match err.kind() {
        LoweringError::Const(ConstEvalError::StepLimitExceeded { limit }) => {
            assert_eq!(*limit, 10_000)
        }
        other => panic!("expected structured step-limit error, got {other:?}"),
    }
    assert_eq!(
        output
            .facts()
            .expect("step-limit fixture must still expose facts")
            .consts()
            .value(HirFactId::Def(width_def)),
        None
    );
}

#[allow(dead_code)]
fn _type_anchor(_: &ConstExprKind) {}
