#[path = "semantic_facts/support.rs"]
mod semantic_facts_support;

use semantic_facts_support::{ExprLookup, def_id, def_id_by_path, expr_id_at, parse_sources};
use syl_sema::ir::const_mir::{ConstEvalEnv, ConstExprKind, ConstMirBuilder, ConstValue};
use syl_sema::{DefinitionKind, HirFactId, SemanticCompiler, SemanticSourceFile};
use syl_span::SourceId;
use syl_syntax::SourceParser;

#[test]
fn software_struct_const_lowering_and_field_eval_regressions_hold() {
    let source = r#"
struct Params {
    width: nat,
    enabled: bool,
}

const DEFAULT: Params = Params { width: 7, enabled: true }
const WIDTH: nat = DEFAULT.width
const ENABLED: bool = DEFAULT.enabled

cell Top(y: out UInt<WIDTH>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("software struct const fixture must parse");
    let files = [file];
    let session = SemanticCompiler::new().session(&files);
    let hir_resolution = session
        .resolve_hir()
        .expect("software struct const fixture must resolve HIR");
    let output = session.check();
    let tir = output
        .tir()
        .expect("software struct const fixture must type-check into TIR");
    let facts = output
        .facts()
        .expect("software struct const fixture must expose semantic facts");
    let hir = tir.design().hir();
    let params_def = def_id(hir, "Params");
    let default_def = def_id(hir, "DEFAULT");
    let width_def = def_id(hir, "WIDTH");
    let enabled_def = def_id(hir, "ENABLED");
    let default_item = hir
        .consts
        .get(&default_def)
        .expect("DEFAULT const item must exist");
    let width_item = hir
        .consts
        .get(&width_def)
        .expect("WIDTH const item must exist");
    let enabled_item = hir
        .consts
        .get(&enabled_def)
        .expect("ENABLED const item must exist");
    let builder = ConstMirBuilder::new(tir.design());
    let lowered_default = builder.lower_const_expr(default_def, &default_item.value);
    let lowered_width = builder.lower_const_expr(width_def, &width_item.value);
    let lowered_enabled = builder.lower_const_expr(enabled_def, &enabled_item.value);
    let default_expr_id = expr_id_at(
        ExprLookup::new(
            source,
            SourceId::new(0),
            "Params { width: 7, enabled: true }",
            0,
            "Params { width: 7, enabled: true }".len(),
        ),
        hir,
    );
    let width_expr_id = expr_id_at(
        ExprLookup::new(
            source,
            SourceId::new(0),
            "DEFAULT.width",
            0,
            "DEFAULT.width".len(),
        ),
        hir,
    );
    let enabled_expr_id = expr_id_at(
        ExprLookup::new(
            source,
            SourceId::new(0),
            "DEFAULT.enabled",
            0,
            "DEFAULT.enabled".len(),
        ),
        hir,
    );

    assert!(matches!(
        hir_resolution
            .resolution()
            .graph()
            .definition_path(params_def)
            .expect("Params definition path must exist")
            .kind(),
        DefinitionKind::Struct
    ));

    match lowered_default.kind() {
        ConstExprKind::Aggregate { kind, fields } => {
            assert_eq!(kind.def(), params_def);
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "width");
            assert!(matches!(fields[0].value().kind(), ConstExprKind::Nat(7)));
            assert_eq!(fields[1].name(), "enabled");
            assert!(matches!(
                fields[1].value().kind(),
                ConstExprKind::Bool(true)
            ));
        }
        other => panic!("expected aggregate lowering for DEFAULT, got {other:?}"),
    }
    match lowered_width.kind() {
        ConstExprKind::Field { base, field } => {
            assert_eq!(field, "width");
            assert!(matches!(
                base.kind(),
                ConstExprKind::Aggregate { kind, .. } if kind.def() == params_def
            ));
        }
        other => panic!("expected field lowering for WIDTH, got {other:?}"),
    }
    match lowered_enabled.kind() {
        ConstExprKind::Field { base, field } => {
            assert_eq!(field, "enabled");
            assert!(matches!(
                base.kind(),
                ConstExprKind::Aggregate { kind, .. } if kind.def() == params_def
            ));
        }
        other => panic!("expected field lowering for ENABLED, got {other:?}"),
    }

    let program = builder
        .build()
        .expect("software struct const MIR program lowering must succeed");
    let mut evaluator = program.evaluator();
    let default_value = evaluator
        .expr_value(
            &lowered_default,
            &mut ConstEvalEnv::with_owner(Some(default_def)),
        )
        .expect("DEFAULT aggregate must evaluate");
    match &default_value {
        ConstValue::Struct(value) => {
            assert_eq!(value.kind().def(), params_def);
            assert_eq!(value.field_value("width"), Some(&ConstValue::Nat(7)));
            assert_eq!(value.field_value("enabled"), Some(&ConstValue::Bool(true)));
        }
        other => panic!("expected evaluated struct const for DEFAULT, got {other:?}"),
    }
    assert_eq!(
        evaluator.recorded_expr_values().get(&default_expr_id),
        Some(&default_value)
    );
    assert_eq!(
        evaluator
            .expr_value(
                &lowered_width,
                &mut ConstEvalEnv::with_owner(Some(width_def))
            )
            .expect("WIDTH field projection must evaluate"),
        ConstValue::Nat(7)
    );
    assert_eq!(
        evaluator
            .expr_value(
                &lowered_enabled,
                &mut ConstEvalEnv::with_owner(Some(enabled_def)),
            )
            .expect("ENABLED field projection must evaluate"),
        ConstValue::Bool(true)
    );
    match facts.consts().value(HirFactId::Def(default_def)) {
        Some(ConstValue::Struct(value)) => {
            assert_eq!(value.kind().def(), params_def);
            assert_eq!(value.field_value("width"), Some(&ConstValue::Nat(7)));
            assert_eq!(value.field_value("enabled"), Some(&ConstValue::Bool(true)));
        }
        other => panic!("expected struct const fact for DEFAULT, got {other:?}"),
    }
    assert_eq!(
        facts.consts().value(HirFactId::Def(width_def)),
        Some(ConstValue::Nat(7))
    );
    assert_eq!(
        facts.consts().value(HirFactId::Def(enabled_def)),
        Some(ConstValue::Bool(true))
    );
    assert_eq!(
        facts.consts().value(HirFactId::Expr(width_expr_id)),
        Some(ConstValue::Nat(7))
    );
    assert_eq!(
        facts.consts().value(HirFactId::Expr(enabled_expr_id)),
        Some(ConstValue::Bool(true))
    );
}

#[test]
fn software_struct_const_imported_same_name_struct_binds_to_correct_def_id() {
    let alpha = r#"
struct Params {
    width: nat,
}
"#;
    let beta = r#"
struct Params {
    enabled: bool,
}
"#;
    let app = r#"
use beta.Params;

const DEFAULT: Params = Params { enabled: true }
const ENABLED: bool = DEFAULT.enabled

cell Top(y: out Bit) {
    y := 1
}
"#;
    let files = parse_sources(&[alpha, beta, app]);
    let session = SemanticCompiler::new().session_sources(vec![
        SemanticSourceFile::new(vec!["alpha".to_string()], &files[0]),
        SemanticSourceFile::new(vec!["beta".to_string()], &files[1]),
        SemanticSourceFile::new(vec!["app".to_string()], &files[2]),
    ]);
    let output = session.check();
    let tir = output
        .tir()
        .expect("cross-package struct fixture must type-check into TIR");
    let facts = output
        .facts()
        .expect("cross-package struct fixture must expose semantic facts");
    let hir = tir.design().hir();
    let alpha_params_def = def_id_by_path(hir, &["alpha", "Params"]);
    let beta_params_def = def_id_by_path(hir, &["beta", "Params"]);
    let default_def = def_id_by_path(hir, &["app", "DEFAULT"]);
    let enabled_def = def_id_by_path(hir, &["app", "ENABLED"]);
    let default_item = hir
        .consts
        .get(&default_def)
        .expect("DEFAULT const item must exist");
    let enabled_item = hir
        .consts
        .get(&enabled_def)
        .expect("ENABLED const item must exist");
    let builder = ConstMirBuilder::new(tir.design());
    let lowered_default = builder.lower_const_expr(default_def, &default_item.value);
    let lowered_enabled = builder.lower_const_expr(enabled_def, &enabled_item.value);
    let enabled_expr_id = expr_id_at(
        ExprLookup::new(
            app,
            SourceId::new(2),
            "DEFAULT.enabled",
            0,
            "DEFAULT.enabled".len(),
        ),
        hir,
    );

    assert_ne!(alpha_params_def, beta_params_def);
    match lowered_default.kind() {
        ConstExprKind::Aggregate { kind, fields } => {
            assert_eq!(kind.def(), beta_params_def);
            assert_ne!(kind.def(), alpha_params_def);
            assert_eq!(fields.len(), 1);
            assert_eq!(fields[0].name(), "enabled");
            assert!(matches!(
                fields[0].value().kind(),
                ConstExprKind::Bool(true)
            ));
        }
        other => panic!("expected aggregate lowering for imported Params, got {other:?}"),
    }
    match lowered_enabled.kind() {
        ConstExprKind::Field { base, field } => {
            assert_eq!(field, "enabled");
            assert!(matches!(
                base.kind(),
                ConstExprKind::Aggregate { kind, .. } if kind.def() == beta_params_def
            ));
        }
        other => panic!("expected field lowering for imported Params, got {other:?}"),
    }

    let program = builder
        .build()
        .expect("cross-package struct const MIR lowering must succeed");
    let mut evaluator = program.evaluator();
    match facts.consts().value(HirFactId::Def(default_def)) {
        Some(ConstValue::Struct(value)) => {
            assert_eq!(value.kind().def(), beta_params_def);
            assert_ne!(value.kind().def(), alpha_params_def);
            assert_eq!(value.field_value("enabled"), Some(&ConstValue::Bool(true)));
            assert_eq!(value.fields().len(), 1);
        }
        other => panic!("expected imported struct const fact for DEFAULT, got {other:?}"),
    }
    assert_eq!(
        evaluator
            .expr_value(
                &lowered_enabled,
                &mut ConstEvalEnv::with_owner(Some(enabled_def)),
            )
            .expect("imported struct field projection must evaluate"),
        ConstValue::Bool(true)
    );
    assert_eq!(
        facts.consts().value(HirFactId::Def(enabled_def)),
        Some(ConstValue::Bool(true))
    );
    assert_eq!(
        facts.consts().value(HirFactId::Expr(enabled_expr_id)),
        Some(ConstValue::Bool(true))
    );
}
