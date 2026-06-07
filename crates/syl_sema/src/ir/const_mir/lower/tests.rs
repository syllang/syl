use super::super::{
    ConstEvalEnv, ConstExpr, ConstExprKind, ConstMirBuilder, ConstMirLoweringContext,
    ConstNamedExpr, ConstStmt, ConstValue,
};
use super::*;
use crate::{
    hir::{HirConstItem, HirDesign, lower::HirResolver},
    tir::{TirDesign, TirType, TypePhaseChecker},
};
use std::sync::Arc;
use syl_hir::DefId;
use syl_span::{SourceId, Span};
use syl_syntax::SourceParser;

struct FakeContext {
    hir: HirDesign,
}

impl ConstMirLoweringContext for FakeContext {
    fn hir(&self) -> &HirDesign {
        &self.hir
    }

    fn is_const_owner(&self, owner: DefId) -> bool {
        self.hir.consts.contains_key(&owner)
    }

    fn expr_resolution(
        &self,
        _owner: DefId,
        expr: &HirBodyExpr,
    ) -> Result<Option<crate::hir::resolve::HirResolution>, crate::CompileError> {
        Ok(self.hir.expr_resolutions.get(&expr.id()).copied())
    }

    fn expr_type(&self, _owner: DefId, _expr: &HirBodyExpr) -> Option<&TirType> {
        None
    }

    fn const_by_def(&self, def: DefId) -> Option<&HirConstItem> {
        self.hir.consts.get(&def)
    }

    fn function_exists(&self, def: DefId) -> bool {
        self.hir.fns.contains_key(&def)
    }

    fn extension_method_call<'a>(
        &self,
        _owner: DefId,
        _callee: &'a HirBodyExpr,
    ) -> Option<(DefId, &'a HirBodyExpr)> {
        None
    }

    fn enum_variant_value(&self, _expr: &HirBodyExpr) -> Option<u64> {
        None
    }
}

#[test]
fn lower_const_expr_uses_context_lookup() {
    let hir = resolve_hir(
        r#"
const answer = 7

fn use_answer() -> nat {
    answer
}
"#,
    );
    let owner = def_id(&hir, "use_answer");
    let lookup_expr = hir
        .fns
        .get(&owner)
        .and_then(|item| item.body.tail.as_ref())
        .expect("fixture function must have a tail expression")
        .clone();
    let lookup_id = lookup_expr.id();
    let ctx = FakeContext { hir };

    let lowered = ConstMirBuilder::with_context(&ctx).lower_const_expr(owner, &lookup_expr);

    match lowered.kind() {
        ConstExprKind::Nat(value) => assert_eq!(*value, 7),
        _ => panic!("expected nat const"),
    }
    assert_eq!(lowered.origin(), Some(lookup_id));
}

#[test]
fn lower_struct_aggregate_expr() {
    let hir = resolve_hir(
        r#"
struct Params {
    width: nat,
    enabled: bool,
}

const params = Params { width: 7, enabled: true }
"#,
    );
    let owner = def_id(&hir, "params");
    let value_expr = hir
        .consts
        .get(&owner)
        .map(|item| item.value.clone())
        .expect("fixture const must exist");
    let lowered =
        ConstMirBuilder::with_context(&FakeContext { hir }).lower_const_expr(owner, &value_expr);

    match lowered.kind() {
        ConstExprKind::Aggregate { kind, fields } => {
            assert_eq!(
                kind.def(),
                def_id(
                    &resolve_hir(
                        r#"
struct Params {
    width: nat,
    enabled: bool,
}

const params = Params { width: 7, enabled: true }
"#,
                    ),
                    "Params"
                )
            );
            assert_eq!(fields.len(), 2);
            assert_eq!(fields[0].name(), "width");
            assert!(matches!(fields[0].value().kind(), ConstExprKind::Nat(7)));
            assert_eq!(fields[1].name(), "enabled");
            assert!(matches!(
                fields[1].value().kind(),
                ConstExprKind::Bool(true)
            ));
        }
        _ => panic!("expected aggregate const"),
    }
}

#[test]
fn lower_struct_field_expr() {
    let hir = resolve_hir(
        r#"
struct Params {
    width: nat,
    enabled: bool,
}

const params = Params { width: 7, enabled: true }

fn use_width() -> nat {
    params.width
}
"#,
    );
    let owner = def_id(&hir, "use_width");
    let field_expr = hir
        .fns
        .get(&owner)
        .and_then(|item| item.body.tail.as_ref())
        .cloned()
        .expect("fixture function must have a field tail expression");
    let lowered =
        ConstMirBuilder::with_context(&FakeContext { hir }).lower_const_expr(owner, &field_expr);

    match lowered.kind() {
        ConstExprKind::Field { base, field } => {
            assert_eq!(field, "width");
            assert!(matches!(base.kind(), ConstExprKind::Aggregate { .. }));
        }
        _ => panic!("expected field projection const"),
    }
}

#[test]
fn lower_fn_struct_field_assignment_rewrites_root_local_assign() {
    let tir = resolve_tir(
        r#"
struct Config {
    width: nat
    enabled: bool
}

fn enable(start: Config) -> Config {
    var cfg: Config = start
    cfg.enabled = true
    return cfg
}
"#,
    );
    let owner = def_id(tir.hir(), "enable");
    let config_def = def_id(tir.hir(), "Config");
    let function_item = tir
        .hir()
        .fns
        .get(&owner)
        .expect("fixture function should exist");
    let (target, value) = match &function_item.body.stmts[1] {
        crate::hir::HirStmt::Assign { target, value, .. } => (target, value),
        _ => panic!("fixture should contain a field assignment statement"),
    };
    let mut exprs = ExprLowerer::new(&tir, owner);
    let (root_expr, _root_local, fields) = exprs
        .local_field_path(target)
        .expect("field assignment should resolve a root-local path");
    assert_eq!(fields, vec!["enabled".to_string()]);
    assert_eq!(
        exprs.struct_kind_for_expr(root_expr),
        Some(ConstStructKind::new(config_def))
    );
    assert!(
        exprs.lower_local_assignment(target, value).is_some(),
        "field assignment should rewrite to a root-local assign before full function lowering"
    );

    let program = ConstMirBuilder::new(&tir)
        .build()
        .expect("const MIR should lower field assignment fixture");
    let function = program
        .function(owner)
        .expect("lowered function should be present");

    assert!(
        !function.is_unsupported(),
        "field assignment should no longer mark const MIR unsupported"
    );

    let rewritten_fields = function
        .blocks
        .iter()
        .flat_map(|block| block.stmts.iter())
        .find_map(|stmt| match stmt {
            ConstStmt::Assign { local, value }
                if local.name() == "cfg"
                    && matches!(
                        value.kind(),
                        ConstExprKind::Aggregate { kind, .. } if kind.def() == config_def
                    ) =>
            {
                match value.kind() {
                    ConstExprKind::Aggregate { fields, .. } => Some(fields),
                    _ => None,
                }
            }
            _ => None,
        })
        .expect("field assignment should rewrite into a root-local aggregate assign");

    let width = rewritten_fields
        .iter()
        .find(|field| field.name() == "width")
        .expect("rewritten aggregate should preserve width");
    match width.value().kind() {
        ConstExprKind::Field { base, field } => {
            assert_eq!(field, "width");
            assert!(matches!(
                base.kind(),
                ConstExprKind::Local(local) if local.name() == "cfg"
            ));
        }
        _ => panic!("untouched fields should be projected from the root local"),
    }

    let enabled = rewritten_fields
        .iter()
        .find(|field| field.name() == "enabled")
        .expect("rewritten aggregate should update enabled");
    assert!(matches!(enabled.value().kind(), ConstExprKind::Bool(true)));

    let config_kind = program
        .struct_kind(config_def)
        .expect("program should retain Config layout");
    let call = ConstExpr::call(
        owner,
        vec![ConstExpr::aggregate(
            config_kind,
            vec![
                ConstNamedExpr::new("width", ConstExpr::nat(7, Span::new(0, 0))),
                ConstNamedExpr::new("enabled", ConstExpr::bool_value(false, Span::new(0, 0))),
            ],
            Span::new(0, 0),
        )],
        Span::new(0, 0),
    );
    let mut evaluator = program.evaluator();
    let result = evaluator
        .expr_value(&call, &mut ConstEvalEnv::default())
        .expect("rewritten field assignment should evaluate");

    match result {
        ConstValue::Struct(value) => {
            assert_eq!(value.kind(), config_kind);
            assert_eq!(value.field_value("width"), Some(&ConstValue::Nat(7)));
            assert_eq!(value.field_value("enabled"), Some(&ConstValue::Bool(true)));
        }
        _ => panic!("function should return updated struct"),
    }
}

fn resolve_hir(source: &str) -> HirDesign {
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("fixture must parse");
    let files = [file];
    HirResolver::new(&files)
        .resolve()
        .expect("fixture must resolve HIR")
}

fn resolve_tir(source: &str) -> TirDesign {
    TypePhaseChecker::new(Arc::new(resolve_hir(source)))
        .check()
        .expect("fixture must type-check")
}

fn def_id(hir: &HirDesign, name: &str) -> DefId {
    hir.defs
        .iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("missing definition {name}"))
        .id
}
