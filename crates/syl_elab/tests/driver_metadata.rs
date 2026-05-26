mod support;

use support::MiddleCompiler;
use syl_elab::ElaborationOutput;
use syl_hw::{HwGuardFrame, HwItem, HwPlace, ParametricHwItem};
use syl_syntax::SourceParser;

struct DriverMetadataHarness {
    middle: MiddleCompiler,
}

impl DriverMetadataHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
        }
    }

    fn compile_output(&self, sources: &[&str]) -> Result<ElaborationOutput, String> {
        let mut files = Vec::new();
        for source in sources {
            let file = SourceParser::new(source).parse_file().map_err(|errs| {
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            })?;
            files.push(file);
        }
        self.middle
            .output_files(&files)
            .map_err(|err| err.to_string())
    }
}

#[test]
fn inline_cell_driver_facts_keep_expansion_origin() {
    let output = DriverMetadataHarness::new()
        .compile_output(&[r#"
cell MakeBit() -> y: Bit {
    y := 1
}

cell Top(y: out Bit) {
    let made = inplace MakeBit()
    y := made
}
"#])
        .expect("inplace cell expansion must still compile");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let fact = metadata
        .driver_facts()
        .iter()
        .find(|fact| {
            fact.module() == "Top"
                && (matches!(fact.target_place(), HwPlace::Ident(name) if name == "made")
                    || matches!(fact.target_place(), HwPlace::Object { name, .. } if name == "made"))
        })
        .expect("inlined cell result drive must be present as a driver fact");
    let expansion = fact
        .origin()
        .expansion_stack()
        .last()
        .expect("inlined cell drive must retain expansion stack");

    assert_eq!(expansion.callable(), "MakeBit");
    assert_eq!(expansion.instance(), "made");
}

#[test]
fn hwir_items_keep_expansion_origin() {
    let output = DriverMetadataHarness::new()
        .compile_output(&[r#"
cell MakeBit() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

cell Top(y: out Bit) {
    let made = inplace MakeBit()
    y := made
}
"#])
        .expect("inline cell expansion must still compile");
    let hwir = output
        .hwir()
        .expect("successful elaboration must produce HW IR");
    let module = hwir
        .modules()
        .iter()
        .find(|module| module.name() == "Top")
        .expect("Top module should be present");
    let origin = module
        .items()
        .iter()
        .find_map(|item| match item {
            ParametricHwItem::Core {
                item: HwItem::SignalDecl { name, .. },
                origin,
            } if name == "made_tmp" => Some(origin),
            _ => None,
        })
        .expect("inlined cell signal item should be present");
    let expansion = origin
        .expansion_stack()
        .last()
        .expect("inlined cell item must retain expansion stack");

    assert_eq!(expansion.callable(), "MakeBit");
    assert_eq!(expansion.instance(), "made");
}

#[test]
fn exposes_driver_metadata_on_hwir() {
    let output = DriverMetadataHarness::new()
        .compile_output(&[r#"
cell Top(y: out Bit) {
    signal tmp: Bit := 1
    y := tmp
}
"#])
        .expect("middle pipeline must produce HWIR with sidecar driver metadata");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    assert!(
        metadata
            .driver_facts()
            .iter()
            .any(|fact| fact.module() == "Top" && fact.target() == "y" && fact.guard() == "root")
    );
    assert!(metadata.driver_facts().iter().any(|fact| {
        fact.module() == "Top"
            && (matches!(fact.target_place(), HwPlace::Ident(name) if name == "y")
                || matches!(fact.target_place(), HwPlace::Object { name, .. } if name == "y"))
    }));
    assert!(
        metadata
            .read_facts()
            .iter()
            .any(|fact| fact.module() == "Top" && fact.source() == "tmp" && fact.guard() == "root")
    );
    assert!(metadata.read_facts().iter().any(|fact| {
        fact.module() == "Top"
            && (matches!(fact.source_place(), HwPlace::Ident(name) if name == "tmp")
                || matches!(fact.source_place(), HwPlace::Object { name, .. } if name == "tmp"))
    }));
    assert!(
        metadata
            .create_facts()
            .iter()
            .any(|fact| fact.module() == "Top" && fact.name() == "tmp")
    );
}

#[test]
fn exposes_structured_driver_guards_on_hwir() {
    let output = DriverMetadataHarness::new()
        .compile_output(&[r#"
cell Top<ENABLE: Bool>(y: out Bit) {
    if ENABLE {
        y := 0
    } else {
        y := 1
    }
}
"#])
        .expect("if/else guarded drivers must be represented as mutually exclusive facts");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let guarded_y: Vec<_> = metadata
        .driver_facts()
        .iter()
        .filter(|fact| fact.module() == "Top" && fact.target() == "y")
        .collect();

    assert_eq!(guarded_y.len(), 2);
    let then_label = guarded_y
        .iter()
        .find_map(|fact| match fact.guard_model().frames() {
            [HwGuardFrame::IfThen { label }] => Some(label),
            _ => None,
        });
    let else_label = guarded_y
        .iter()
        .find_map(|fact| match fact.guard_model().frames() {
            [HwGuardFrame::IfElse { label }] => Some(label),
            _ => None,
        });

    assert!(then_label.is_some());
    assert_eq!(then_label, else_label);
}
