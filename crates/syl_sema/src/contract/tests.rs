use super::{declaration_contracts, hardware_contract_for, signature_stub_file};
use syl_syntax::parser::SourceParser;

fn parse(source: &str) -> syl_syntax::AstFile {
    SourceParser::new(source)
        .parse_file()
        .expect("contract fixture must parse")
}

#[test]
fn body_only_edit_keeps_declaration_projection() {
    let before = parse("fn helper() -> nat { return 1 }\n");
    let after = parse("fn helper() -> nat { return 100 }\n");
    let module = ["body_b".to_string()];
    assert_eq!(
        declaration_contracts(&module, "fn helper() -> nat { return 1 }\n", &before),
        declaration_contracts(&module, "fn helper() -> nat { return 100 }\n", &after)
    );
    assert_eq!(signature_stub_file(&before), signature_stub_file(&after));
}

#[test]
fn signature_edit_changes_declaration_projection() {
    let before = parse("fn helper() -> nat { return 1 }\n");
    let after = parse("fn helper(x: nat) -> nat { return 1 }\n");
    assert_ne!(signature_stub_file(&before), signature_stub_file(&after));
}

#[test]
fn const_value_edit_changes_declaration_projection() {
    let before = parse("const WIDTH: nat = 1\n");
    let after = parse("const WIDTH: nat = 2\n");
    let module = ["nums".to_string()];
    assert_ne!(
        declaration_contracts(&module, "const WIDTH: nat = 1\n", &before),
        declaration_contracts(&module, "const WIDTH: nat = 2\n", &after)
    );
}

#[test]
fn import_resolution_reads_module_declarations() {
    use crate::{ContractProvider, SemanticSession, SemanticSourceFile};

    struct Named {
        signature: String,
    }

    impl ContractProvider for Named {
        fn declaration(&self, _key: &super::SymbolKey) -> Option<super::DeclarationContract> {
            None
        }

        fn hardware(&self, _key: &super::SymbolKey) -> Option<super::HardwareContract> {
            None
        }

        fn declarations(&self, module: &[String]) -> Vec<super::DeclarationContract> {
            panic!("import resolution must read {module:?} by name, not the whole module");
        }

        fn imported_declaration(
            &self,
            module: &[String],
            name: &str,
        ) -> Option<super::DeclarationContract> {
            assert_eq!(module, &["lib".to_string()]);
            assert_eq!(name, "helper");
            Some(super::DeclarationContract::new(
                super::SymbolKey::new(module.to_vec(), super::SymbolKind::Fn, name, 0),
                None,
                self.signature.clone(),
                None,
            ))
        }
    }

    fn codes(signature: &str) -> Vec<String> {
        let lib = parse("fn helper() -> nat { return 1 }\n");
        let app = parse("use lib.helper\nfn run() -> nat { return helper() }\n");
        let session = SemanticSession::new_sources(vec![
            SemanticSourceFile::new(vec!["lib".to_string()], &lib),
            SemanticSourceFile::new(vec!["app".to_string()], &app),
        ]);
        session
            .resolve_hir_partial_with_contracts(&Named {
                signature: signature.to_string(),
            })
            .diagnostics()
            .iter()
            .filter_map(|diagnostic| diagnostic.code.clone())
            .collect()
    }

    let mismatched = codes("fn other() -> nat { }");
    assert!(
        mismatched
            .iter()
            .any(|code| code == "E_MIDDLE_UNRESOLVED_NAME"),
        "a signature that does not name helper must not resolve: {mismatched:?}"
    );
    let matched = codes("fn helper() -> nat { return 1 }");
    assert!(
        matched
            .iter()
            .all(|code| code != "E_MIDDLE_UNRESOLVED_NAME"),
        "a signature that names helper must resolve: {matched:?}"
    );
}

#[test]
fn storage_edit_changes_hardware_contract_only() {
    let before = parse("cell Buf(y: out Bit) { y := 1 }\n");
    let after = parse("cell Buf(y: out Bit) { reg x: Bit; y := x }\n");
    let module = ["buf".to_string()];
    assert_eq!(signature_stub_file(&before), signature_stub_file(&after));
    let before_hw = hardware_contract_for(&module, &before);
    let after_hw = hardware_contract_for(&module, &after);
    assert!(!before_hw[0].has_storage);
    assert!(after_hw[0].has_storage);
}
