use super::*;

#[test]
fn reserved_names_extern_symbols_and_secondary_collisions_are_order_independent() {
    fn module(package: &str, name: &str, external: bool) -> EirModule {
        let module = if external {
            EirModule::new_extern(name, vec![], vec![])
        } else {
            EirModule::new(name, vec![], vec![], vec![])
        };
        module.with_canonical_path(vec![package.into(), name.into()])
    }
    let mut modules = vec![
        module("vendor", "Fifo", true),
        module("a", "Fifo", false),
        module("user", "a__Fifo", false),
        module("user", "wire", false),
        module("b", "Fifo", false),
    ];
    let names = |input| {
        EirModule::disambiguate(input)
            .into_iter()
            .map(|m| (m.canonical_path, m.name))
            .collect::<BTreeMap<_, _>>()
    };
    let forward = names(modules.clone());
    modules.reverse();
    assert_eq!(forward, names(modules));
    let name = |pkg: &str, leaf: &str| &forward[&vec![pkg.to_owned(), leaf.to_owned()]];
    assert_eq!(name("vendor", "Fifo"), "Fifo");
    assert_eq!(name("user", "a__Fifo"), "a__Fifo");
    assert_eq!(name("user", "wire"), "syl_wire");
    assert_eq!(name("b", "Fifo"), "b__Fifo");
    assert_eq!(name("a", "Fifo"), "a__Fifo__h4ecfa7aa");
}
