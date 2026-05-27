use super::*;
use crate::hir::{HirDesign, HirExprNode, lower::HirResolver};
use std::collections::BTreeMap;
use syl_hir::{DefId, ExprId};
use syl_span::SourceId;
use syl_syntax::SourceParser;

struct FakeContext {
    hir: HirDesign,
    enum_values: BTreeMap<ExprId, u64>,
}

impl MapLoweringContext for FakeContext {
    fn hir(&self) -> &HirDesign {
        &self.hir
    }

    fn expr_resolution(
        &self,
        _owner: DefId,
        _expr: &HirBodyExpr,
    ) -> Result<Option<crate::hir::resolve::HirResolution>, crate::CompileError> {
        Ok(None)
    }

    fn expr_type(&self, _owner: DefId, _expr: &HirBodyExpr) -> Option<&TirType> {
        None
    }

    fn builtin_intrinsic(&self, _owner: DefId, callee: &HirBodyExpr) -> Option<BuiltinIntrinsic> {
        match &callee.node {
            HirExprNode::Ident(name) if name == "z" => Some(BuiltinIntrinsic::HighZ),
            _ => None,
        }
    }

    fn extension_method_call<'a>(
        &self,
        _owner: DefId,
        _callee: &'a HirBodyExpr,
    ) -> Option<(DefId, &'a HirBodyExpr, Vec<TirGenericArg>)> {
        None
    }

    fn enum_variant_value(&self, expr: &HirBodyExpr) -> Option<u64> {
        self.enum_values.get(&expr.id()).copied()
    }

    fn def_kind(&self, _def: DefId) -> Option<HirDefKind> {
        None
    }

    fn def_name(&self, _def: DefId) -> Option<&str> {
        None
    }
}

#[test]
fn lowering_uses_context_builtin_and_enum_values() {
    let hir = resolve_hir(
        r#"
enum Color {
    Red,
}

map highz() -> Bit =
    z()

map red() -> Color =
    Color.Red
"#,
    );
    let highz_owner = def_id(&hir, "highz");
    let red_owner = def_id(&hir, "red");
    let builtin_expr = hir
        .maps
        .get(&highz_owner)
        .expect("highz map must exist")
        .body
        .clone();
    let variant_expr = hir
        .maps
        .get(&red_owner)
        .expect("red map must exist")
        .body
        .clone();
    let ctx = FakeContext {
        hir,
        enum_values: BTreeMap::from([(variant_expr.id(), 17)]),
    };
    let builder = MapIrBuilder::with_context(&ctx);

    assert!(matches!(
        builder.lower_expr(highz_owner, &builtin_expr).unwrap(),
        MapExpr::BuiltinHighZ
    ));
    assert!(matches!(
        builder.lower_expr(red_owner, &variant_expr).unwrap(),
        MapExpr::Int(17)
    ));
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

fn def_id(hir: &HirDesign, name: &str) -> DefId {
    hir.defs
        .iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("missing definition {name}"))
        .id
}
