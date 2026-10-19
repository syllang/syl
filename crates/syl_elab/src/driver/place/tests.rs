use super::{DriverExpr, DriverObjectTable, DriverPlaceResolver};
use crate::eir::{EirExpr, EirPlace};

#[test]
fn driver_expr_rejects_unsupported_eir_expr() {
    let expr = EirExpr::mux(
        EirExpr::ident("cond"),
        EirExpr::unsupported("bad branch"),
        EirExpr::Int(0),
    );

    assert!(DriverExpr::from_eir_expr(&expr).is_err());
}

#[test]
fn place_resolver_rejects_unsupported_projection_index() {
    let mut objects = DriverObjectTable::new();
    objects.intern("Top", "word");
    let resolver = DriverPlaceResolver::new("Top", &objects);
    let place = EirPlace::Index {
        base: Box::new(EirPlace::Ident("word".to_string())),
        index: EirExpr::unsupported("bad index"),
    };

    assert!(resolver.resolve_place(&place).is_err());
}

#[test]
fn place_resolver_rejects_unknown_ident() {
    let objects = DriverObjectTable::new();
    let resolver = DriverPlaceResolver::new("Top", &objects);
    let place = EirPlace::Ident("missing".to_string());

    let error = resolver
        .resolve_place(&place)
        .expect_err("unknown EIR place root must not become a string fallback");

    assert_eq!(
        error,
        super::DriverPlaceError::UnknownObject {
            module: "Top".to_string(),
            name: "missing".to_string()
        }
    );
}
