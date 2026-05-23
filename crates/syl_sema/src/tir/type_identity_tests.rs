use super::{BindingRef, TypePhaseChecker};
use crate::HirResolver;
use crate::hir::{HirDesign, HirLocalKind};
use syl_hir::{DefId, LocalId};
use syl_span::Span;
use syl_syntax::{AstFile, Expr, GenericParam, Item, MapItem, Param, TypeExpr};

#[test]
fn owner_generic_named_types_use_hir_local_identity() {
    let first_generic_span = Span::new(10, 11);
    let first_param_span = Span::new(20, 21);
    let second_generic_span = Span::new(60, 61);
    let second_param_span = Span::new(70, 71);
    let files = vec![AstFile::new(vec![
        Item::Map(
            MapItem::builder("First".to_string(), Expr::Int(1, Span::new(30, 31)))
                .generics(vec![GenericParam::new(
                    "T".to_string(),
                    None,
                    None,
                    first_generic_span,
                )])
                .params(vec![Param::new(
                    "x".to_string(),
                    None,
                    TypeExpr::Path(vec!["T".to_string()], first_param_span),
                    first_param_span,
                )])
                .span(Span::new(0, 31))
                .build(),
        ),
        Item::Map(
            MapItem::builder("Second".to_string(), Expr::Int(1, Span::new(80, 81)))
                .generics(vec![GenericParam::new(
                    "T".to_string(),
                    None,
                    None,
                    second_generic_span,
                )])
                .params(vec![Param::new(
                    "x".to_string(),
                    None,
                    TypeExpr::Path(vec!["T".to_string()], second_param_span),
                    second_param_span,
                )])
                .span(Span::new(50, 81))
                .build(),
        ),
    ])];
    let hir = HirResolver::new(&files)
        .resolve()
        .expect("HIR should resolve owner generic type params");
    let first = TestMapLocals::from_hir(&hir, "First");
    let second = TestMapLocals::from_hir(&hir, "Second");
    let tir = TypePhaseChecker::new(std::sync::Arc::new(hir))
        .check()
        .expect("TIR should accept owner generic type params");

    assert_eq!(
        tir.binding_type_generic_local(BindingRef::Local(first.param)),
        Some(first.generic)
    );
    assert_eq!(
        tir.binding_type_generic_local(BindingRef::Local(second.param)),
        Some(second.generic)
    );
    assert_ne!(
        tir.binding_type_id(BindingRef::Local(first.param)),
        tir.binding_type_id(BindingRef::Local(second.param))
    );
}

#[test]
fn fixed_width_builtin_types_keep_distinct_type_ids() {
    let types = TestTypeExprFactory::new(Span::new(10, 11));
    let files = vec![AstFile::new(vec![Item::Map(
        MapItem::builder("Kinds".to_string(), Expr::Int(1, Span::new(80, 81)))
            .generics(vec![GenericParam::new(
                "W".to_string(),
                Some(TypeExpr::Path(vec!["Nat".to_string()], Span::new(1, 2))),
                None,
                Span::new(2, 3),
            )])
            .params(vec![
                Param::new(
                    "u".to_string(),
                    None,
                    types.generic("UInt", "W"),
                    Span::new(20, 21),
                ),
                Param::new(
                    "b".to_string(),
                    None,
                    types.generic("Bits", "W"),
                    Span::new(30, 31),
                ),
                Param::new(
                    "s".to_string(),
                    None,
                    types.generic("SInt", "W"),
                    Span::new(40, 41),
                ),
            ])
            .span(Span::new(0, 81))
            .build(),
    )])];
    let hir = HirResolver::new(&files)
        .resolve()
        .expect("HIR should resolve fixed-width builtin type fixture");
    let locals = TestItemLocals::new(&hir, "Kinds");
    let uint = BindingRef::Local(locals.local("u"));
    let bits = BindingRef::Local(locals.local("b"));
    let sint = BindingRef::Local(locals.local("s"));
    let tir = TypePhaseChecker::new(std::sync::Arc::new(hir))
        .check()
        .expect("TIR should accept fixed-width builtin type fixture");

    assert_ne!(tir.binding_type_id(uint), tir.binding_type_id(bits));
    assert_ne!(tir.binding_type_id(uint), tir.binding_type_id(sint));
    assert_ne!(tir.binding_type_id(bits), tir.binding_type_id(sint));
    assert_eq!(tir.binding_type_label(uint), Some("UInt<W>".to_string()));
    assert_eq!(tir.binding_type_label(bits), Some("Bits<W>".to_string()));
    assert_eq!(tir.binding_type_label(sint), Some("SInt<W>".to_string()));
}

#[test]
fn clock_and_reset_domains_keep_distinct_type_ids() {
    let types = TestTypeExprFactory::new(Span::new(100, 101));
    let domain_kind = Some(TypeExpr::Path(vec!["Domain".to_string()], Span::new(1, 2)));
    let files = vec![AstFile::new(vec![Item::Map(
        MapItem::builder("Domains".to_string(), Expr::Int(1, Span::new(90, 91)))
            .generics(vec![
                GenericParam::new("D".to_string(), domain_kind.clone(), None, Span::new(2, 3)),
                GenericParam::new("E".to_string(), domain_kind, None, Span::new(4, 5)),
            ])
            .params(vec![
                Param::new(
                    "clk_d".to_string(),
                    None,
                    types.generic("Clock", "D"),
                    Span::new(20, 21),
                ),
                Param::new(
                    "clk_e".to_string(),
                    None,
                    types.generic("Clock", "E"),
                    Span::new(30, 31),
                ),
                Param::new(
                    "rst_d".to_string(),
                    None,
                    types.generic("Reset", "D"),
                    Span::new(40, 41),
                ),
                Param::new(
                    "rst_e".to_string(),
                    None,
                    types.generic("Reset", "E"),
                    Span::new(50, 51),
                ),
            ])
            .span(Span::new(0, 91))
            .build(),
    )])];
    let hir = HirResolver::new(&files)
        .resolve()
        .expect("HIR should resolve domain-sensitive type fixture");
    let locals = TestItemLocals::new(&hir, "Domains");
    let clk_d = BindingRef::Local(locals.local("clk_d"));
    let clk_e = BindingRef::Local(locals.local("clk_e"));
    let rst_d = BindingRef::Local(locals.local("rst_d"));
    let rst_e = BindingRef::Local(locals.local("rst_e"));
    let tir = TypePhaseChecker::new(std::sync::Arc::new(hir))
        .check()
        .expect("TIR should accept domain-sensitive type fixture");

    assert_ne!(tir.binding_type_id(clk_d), tir.binding_type_id(clk_e));
    assert_ne!(tir.binding_type_id(rst_d), tir.binding_type_id(rst_e));
    assert_ne!(tir.binding_type_id(clk_d), tir.binding_type_id(rst_d));
    assert_eq!(tir.binding_type_label(clk_d), Some("Clock<D>".to_string()));
    assert_eq!(tir.binding_type_label(clk_e), Some("Clock<E>".to_string()));
    assert_eq!(tir.binding_type_label(rst_d), Some("Reset<D>".to_string()));
    assert_eq!(tir.binding_type_label(rst_e), Some("Reset<E>".to_string()));
}

struct TestMapLocals {
    generic: LocalId,
    param: LocalId,
}

impl TestMapLocals {
    fn from_hir(hir: &crate::hir::HirDesign, name: &str) -> Self {
        let owner = hir
            .defs
            .iter()
            .find(|def| def.name == name)
            .expect("map definition should exist")
            .id;
        let generic = hir
            .locals
            .iter()
            .find(|local| {
                local.owner == owner
                    && local.name == "T"
                    && matches!(local.kind, HirLocalKind::Generic)
            })
            .expect("generic local should exist")
            .id;
        let param = hir
            .locals
            .iter()
            .find(|local| {
                local.owner == owner
                    && local.name == "x"
                    && matches!(local.kind, HirLocalKind::Param)
            })
            .expect("param local should exist")
            .id;
        Self { generic, param }
    }
}

#[non_exhaustive]
struct TestItemLocals<'a> {
    hir: &'a HirDesign,
    owner: DefId,
}

impl<'a> TestItemLocals<'a> {
    fn new(hir: &'a HirDesign, item: &str) -> Self {
        let owner = hir
            .defs
            .iter()
            .find(|def| def.name == item)
            .expect("test item definition should exist")
            .id;
        Self { hir, owner }
    }

    fn local(&self, name: &str) -> LocalId {
        self.hir
            .locals
            .iter()
            .find(|local| local.owner == self.owner && local.name == name)
            .expect("test local should exist")
            .id
    }
}

#[non_exhaustive]
struct TestTypeExprFactory {
    span: Span,
}

impl TestTypeExprFactory {
    fn new(span: Span) -> Self {
        Self { span }
    }

    fn generic(&self, base: &str, arg: &str) -> TypeExpr {
        TypeExpr::Generic {
            base: Box::new(TypeExpr::Path(vec![base.to_string()], self.span)),
            args: vec![TypeExpr::Path(vec![arg.to_string()], self.span)],
            span: self.span,
        }
    }
}
