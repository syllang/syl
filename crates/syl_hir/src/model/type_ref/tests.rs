use std::collections::HashMap;

use syl_span::Span;

use super::{MirConstExpr, MirTypeRef};

#[test]
fn const_substitution_preserves_multi_segment_type_paths() {
    let span = Span::new(4, 9);
    let expr = MirConstExpr::ident_expr("N".to_string(), span);

    let mut replacements = HashMap::new();
    replacements.insert(
        "N".to_string(),
        MirTypeRef::path_type(vec!["pkg".to_string(), "WIDTH".to_string()], span),
    );

    let substituted = expr.subst_type_vars(&replacements);

    assert_eq!(substituted, expr);
    assert_eq!(substituted.ident(), Some("N"));
}

#[test]
fn const_substitution_still_accepts_single_segment_type_paths() {
    let span = Span::new(11, 17);
    let expr = MirConstExpr::ident_expr("N".to_string(), span);

    let mut replacements = HashMap::new();
    replacements.insert(
        "N".to_string(),
        MirTypeRef::path_type(vec!["8".to_string()], span),
    );

    let substituted = expr.subst_type_vars(&replacements);

    assert_eq!(substituted, MirConstExpr::nat(8, span));
    assert_eq!(substituted.nat_value(), Some(8));
}
