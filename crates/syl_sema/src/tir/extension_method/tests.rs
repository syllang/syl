use super::*;
use crate::hir::{HirDef, HirDefKind, HirDesign};
use syl_hir::HirPath;
use syl_span::Span;

#[test]
fn resolver_reports_ambiguous_extension_methods() {
    let receiver = DefId::new(0);
    let first = DefId::new(1);
    let second = DefId::new(2);
    let owner = DefId::new(3);
    let mut hir = HirDesign::empty();
    *hir.defs_mut() = vec![
        def(receiver, "Word", "pkg.Word", HirDefKind::Bundle),
        def(first, "flag", "pkg.flag_a", HirDefKind::Map),
        def(second, "flag", "pkg.flag_b", HirDefKind::Map),
        def(owner, "Top", "pkg.Top", HirDefKind::Cell),
    ];
    hir.register_extension_method(receiver, "flag".to_string(), first);
    hir.register_extension_method(receiver, "flag".to_string(), second);

    match resolve_extension_method(&hir, owner, receiver, "flag") {
        ExtensionMethodResolution::Ambiguous(candidates) => {
            assert_eq!(candidates, vec![first, second]);
        }
        _ => panic!("resolver must preserve ambiguous extension method candidates"),
    }
}

fn def(id: DefId, name: &str, path: &str, kind: HirDefKind) -> HirDef {
    HirDef::new(
        id,
        name.to_string(),
        HirPath::new(path.split('.').map(str::to_string).collect()),
        kind,
        Span::default(),
    )
}
