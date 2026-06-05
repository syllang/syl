mod support;

use support::MiddleCompiler;
use syl_hw::{HwItem, ParametricHwItem};
use syl_syntax::SourceParser;

#[test]
fn local_mutation_numbers_bare_place_instances_across_if_and_for() {
    let instance_ids = compile_instance_param_values(
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
        "ID",
    )
    .expect("numbering fixture must elaborate");

    assert_eq!(
        instance_ids,
        vec!["0".to_string(), "1".to_string(), "2".to_string()],
        "local mutation must assign continuous IDs across symbolic if and later static for"
    );
}

#[test]
fn symbolic_if_unknown_mutable_nat_generic_actual_stays_symbolic() {
    let values = compile_instance_param_values(
        r#"
cell Leaf<X: nat>() -> y: Bit {
    y := 1
}

cell Top<COND: bool>(y: out Bit) {
    var x: nat = 0
    if COND {
        x = 1
    }
    place Leaf<x>()
    y := 0
}
 "#,
        "X",
    )
    .expect(
        "unknown mutable nat generic actual should remain conservative instead of concretizing",
    );

    assert_eq!(
        values,
        vec!["x".to_string()],
        "unknown single-branch mutable nat must not collapse to a guessed constant"
    );
}

#[test]
fn symbolic_if_else_unknown_mutable_nat_generic_actual_stays_symbolic() {
    let values = compile_instance_param_values(
        r#"
cell Leaf<X: nat>() -> y: Bit {
    y := 1
}

cell Top<COND: bool>(y: out Bit) {
    var x: nat = 0
    if COND {
        x = 2
    } else {
        x = 5
    }
    place Leaf<x>()
    y := 0
}
 "#,
        "X",
    )
    .expect("divergent mutable nat branches should remain conservative instead of concretizing");

    assert_eq!(
        values,
        vec!["x".to_string()],
        "divergent mutable nat branches must not collapse to an arbitrary constant"
    );
}

fn compile_instance_param_values(source: &str, param_name: &str) -> Result<Vec<String>, String> {
    let file = SourceParser::new(source)
        .parse_file()
        .map_err(|error| format!("fixture must parse: {error:?}"))?;
    let middle = MiddleCompiler::new();
    let hir = middle
        .session(std::slice::from_ref(&file))
        .resolve_hir()
        .map_err(|error| format!("fixture must resolve HIR: {error:?}"))?;
    let tir_output = hir.check_tir_partial();
    if !tir_output.diagnostics().is_empty() {
        return Err(format!(
            "fixture must pass partial TIR checking: {:?}",
            tir_output.diagnostics()
        ));
    }
    let output = middle
        .output_files(&[file])
        .map_err(|error| format!("fixture must elaborate: {error:?}"))?;
    let hwir = output.hwir().ok_or_else(|| {
        format!(
            "successful elaboration must produce HWIR: {:?}",
            output.diagnostics()
        )
    })?;
    let top = hwir
        .modules()
        .iter()
        .find(|module| module.name() == "Top")
        .ok_or_else(|| "Top module must exist".to_string())?;
    let mut values = Vec::new();
    collect_instance_param_values_impl(top.items(), param_name, &mut values);
    Ok(values)
}

fn collect_instance_param_values_impl(
    items: &[ParametricHwItem],
    param_name: &str,
    values: &mut Vec<String>,
) {
    for item in items {
        match item {
            ParametricHwItem::Core {
                item: HwItem::Instance(instance),
                ..
            } => {
                if let Some(value) = param_value(instance, param_name) {
                    values.push(value.to_string());
                }
            }
            ParametricHwItem::StaticIf {
                then_items,
                else_items,
                ..
            } => {
                collect_instance_param_values_impl(then_items, param_name, values);
                collect_instance_param_values_impl(else_items, param_name, values);
            }
            ParametricHwItem::StaticFor { items, .. } => {
                collect_instance_param_values_impl(items, param_name, values);
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
