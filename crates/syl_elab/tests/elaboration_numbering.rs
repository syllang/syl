mod support;

use support::MiddleCompiler;
use syl_hw::{HwItem, ParametricHwItem};
use syl_syntax::SourceParser;

#[test]
fn local_mutation_numbers_bare_place_instances_across_if_and_for() {
    let file = SourceParser::new(
        r#"
cell Leaf<ID: nat>() -> y: Bit {
    y := 1
}

cell Top<ENABLE: bool>(y: out Bit) {
    var next_id: nat = 0

    if ENABLE {
        const id: nat = next_id
        next_id = next_id + 1
        place Leaf<id>()
    }

    for lane in 0..2 {
        const id: nat = next_id
        next_id = next_id + 1
        place Leaf<id>()
    }

    y := 0
}
 "#,
    )
    .parse_file()
    .expect("numbering fixture must parse");
    let middle = MiddleCompiler::new();
    let hir = middle
        .session(&[file.clone()])
        .resolve_hir()
        .expect("numbering fixture must resolve HIR");
    let tir_output = hir.check_tir_partial();
    assert!(
        tir_output.diagnostics().is_empty(),
        "TIR checking must accept elaboration numbering fixture: {:?}",
        tir_output.diagnostics()
    );
    let tir = tir_output
        .stage()
        .expect("partial TIR analysis must keep the typed stage");
    let output = middle
        .output_files(&[file])
        .expect("bare place statements in elaboration control flow must compile");
    let _ = tir;
    let hwir = output.hwir().unwrap_or_else(|| {
        panic!(
            "successful elaboration must produce HWIR: {:?}",
            output.diagnostics()
        )
    });
    let top = hwir
        .modules()
        .iter()
        .find(|module| module.name() == "Top")
        .expect("Top module must exist");

    let mut instance_ids = Vec::new();
    collect_instance_ids(top.items(), &mut instance_ids);

    assert_eq!(
        instance_ids,
        vec!["0".to_string(), "1".to_string(), "2".to_string()],
        "local mutation must assign continuous IDs across symbolic if and later static for"
    );
}

fn collect_instance_ids(items: &[ParametricHwItem], instance_ids: &mut Vec<String>) {
    for item in items {
        match item {
            ParametricHwItem::Core {
                item: HwItem::Instance(instance),
                ..
            } => {
                if let Some(id) = param_value(instance, "ID") {
                    instance_ids.push(id.to_string());
                }
            }
            ParametricHwItem::StaticIf {
                then_items,
                else_items,
                ..
            } => {
                collect_instance_ids(then_items, instance_ids);
                collect_instance_ids(else_items, instance_ids);
            }
            ParametricHwItem::StaticFor { items, .. } => {
                collect_instance_ids(items, instance_ids);
            }
            _ => {}
        }
    }
}

fn param_value<'a>(instance: &'a syl_hw::HwInstance, name: &str) -> Option<&'a str> {
    instance
        .params()
        .iter()
        .find(|param| param.name() == name)
        .map(|param| param.value())
}
