use super::*;

#[test]
fn cross_package_definitions_imports_and_named_ports_are_exact() {
    let util = "const WIDTH = 8; bundle Word { bits: UInt<WIDTH> } fn helper(x: nat) -> nat { x } extern cell Sub(x: in Bit)
";
    let app = "use util.WIDTH; use util.Word; use util.helper; use util.Sub; cell Top(x: in Bit, data: in Word) { signal a: UInt<WIDTH>; place Sub(x: x); } const A = helper(1);";
    let (s, uris) = workspace(&[("util", util), ("app", app)]);
    assert!(s.diagnostics().is_empty(), "{:?}", s.diagnostics());
    for (name, marker, count) in [
        ("WIDTH", "WIDTH =", 4),
        ("Word", "Word {", 3),
        ("helper", "helper(x", 3),
        ("Sub", "Sub(x", 3),
    ] {
        let p = position(util, marker);
        let refs = s.references_at(&uris[0], p, true);
        assert_eq!(refs.len(), count, "{name}: {refs:?}");
        assert!(refs.iter().all(|loc| source_at(&s, loc) == name));
        assert_eq!(s.references_at(&uris[0], p, false).len(), count - 1);
        let edit = s
            .rename_at(&uris[0], p, &format!("New{name}"))
            .unwrap()
            .unwrap();
        assert_eq!(edit.changes.values().map(Vec::len).sum::<usize>(), count);
        let use_pos = position(app, &format!("{name};"));
        assert_eq!(
            s.prepare_rename_at(&uris[1], use_pos).unwrap().placeholder,
            name
        );
    }
    let port = position(util, "x: in");
    let refs = s.references_at(&uris[0], port, true);
    assert_eq!(refs.len(), 2, "{refs:?}");
    assert!(refs.iter().all(|loc| source_at(&s, loc) == "x"));
    let named = position(app, "x: x");
    assert_eq!(s.references_at(&uris[1], named, true), refs);
    let edits = s.rename_at(&uris[1], named, "input").unwrap().unwrap();
    assert_eq!(edits.changes.values().map(Vec::len).sum::<usize>(), 2);
}

#[test]
fn registers_generics_and_shadowed_locals_keep_identity() {
    let text = "cell C<W: nat>(x: in UInt<W>, y: out UInt<W>) { reg r: UInt<W>; next r := x; y := r; } fn f(x: nat) -> nat { let a = x; { let a = 2; a }; a }";
    let (s, uri) = snapshot(text);
    let refs = s.references_at(&uri, position(text, "r: UInt"), true);
    assert_eq!(refs.len(), 3, "{refs:?}");
    assert!(refs.iter().all(|loc| source_at(&s, loc) == "r"));
    assert_eq!(s.references_at(&uri, position(text, "r :="), true), refs);
    let generics = s.references_at(&uri, position(text, "W: nat"), true);
    assert_eq!(generics.len(), 4, "{generics:?}");
    let outer = s.references_at(&uri, position(text, "a = x"), true);
    let inner = s.references_at(&uri, position(text, "a = 2"), true);
    assert_eq!(outer.len(), 2, "{outer:?}");
    assert_eq!(inner.len(), 2, "{inner:?}");
    assert!(outer.iter().all(|r| !inner.contains(r)));
}

#[test]
fn rename_rejects_capture_import_collisions_and_non_symbols() {
    let util = "const original = 1; const duplicate = 2;";
    let app = "use util.original; const occupied = 3; fn f() -> nat { original }";
    let (s, uris) = workspace(&[("util", util), ("app", app)]);
    let p = position(util, "original");
    assert!(s.rename_at(&uris[0], p, "duplicate").is_err());
    assert!(s.rename_at(&uris[0], p, "occupied").is_err());
    assert!(
        s.rename_at(&uris[0], p, "original")
            .unwrap()
            .unwrap()
            .changes
            .is_empty()
    );
    let text = "const existing = 1; fn f(x: nat) -> nat { let value = existing; value } // value\n";
    let (s, uri) = snapshot(text);
    assert!(
        s.rename_at(&uri, position(text, "value ="), "existing")
            .is_err()
    );
    for marker in ["const", "nat", "1;", "// value"] {
        assert!(
            s.prepare_rename_at(&uri, position(text, marker)).is_none(),
            "{marker}"
        );
    }
    let comment = SourcePosition::new(0, text.find("// value").unwrap() + 4);
    assert!(s.prepare_rename_at(&uri, comment).is_none());
    let text = "bundle B { x: Bit } fn method(this self: B) -> nat { 1 }";
    let (s, uri) = snapshot(text);
    assert!(s.prepare_rename_at(&uri, position(text, "this")).is_none());
}

#[test]
fn importing_another_symbol_with_new_name_is_a_collision() {
    let (s, uris) = workspace(&[
        ("one", "const Original = 1;"),
        ("two", "const Taken = 2;"),
        (
            "app",
            "use one.Original; use two.Taken; const C = Original;",
        ),
    ]);
    assert!(
        s.rename_at(&uris[0], SourcePosition::new(0, 6), "Taken")
            .is_err()
    );
}
