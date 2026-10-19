mod support;

use support::MiddleCompiler;
use syl_elab::ElaborationOutput;
use syl_hw::HwPlace as HardwarePlace;
use syl_sema::summary::cell::{
    CellBoundarySummary, CellSummaryDeclaration, CellSummaryRegistry, HwPlace as SummaryPlace,
};
use syl_span::{SourceId, Span};
use syl_syntax::SourceParser;

struct CellSummaryHarness {
    middle: MiddleCompiler,
}

impl CellSummaryHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
        }
    }

    fn compile_output(&self, source: &str) -> Result<ElaborationOutput, String> {
        let file = SourceParser::new(source).parse_file().map_err(|errs| {
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        self.middle
            .output_files(&[file])
            .map_err(|err| err.to_string())
    }
}

#[test]
fn inline_cell_expansion_exports_public_driver_summary() {
    let output = CellSummaryHarness::new()
        .compile_output(
            r#"
cell MakePair() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

cell Top(y: out Bit) {
    let made = inplace MakePair()
    y := made
}
"#,
        )
        .expect("inplace cell expansion must compile into parent module");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let summary = metadata
        .cell_summaries()
        .iter()
        .find(|summary| summary.callable() == "MakePair" && summary.instance() == "made")
        .expect("inlined cell must export an instantiation summary");

    assert!(summary.drives().iter().any(|place| {
        matches!(place, HardwarePlace::Ident(name) if name == "made")
            || matches!(place, HardwarePlace::Object { name, .. } if name == "made")
    }));
    assert!(summary.reads().iter().any(|place| {
        matches!(place, HardwarePlace::Ident(name) if name == "made_tmp")
            || matches!(place, HardwarePlace::Object { name, .. } if name == "made_tmp")
    }));
    assert!(summary.creates().iter().any(|name| name == "made_tmp"));
    assert_eq!(
        summary
            .origin()
            .expansion_stack()
            .last()
            .map(|expansion| expansion.callable()),
        Some("MakePair")
    );
}

#[test]
fn hierarchical_source_cell_exports_signature_derived_driver_summary() {
    let output = CellSummaryHarness::new()
        .compile_output(
            r#"
cell Link(x: in Bit, y: out Bit) {
    y := x
}

cell Top(x: in Bit, y: out Bit) {
    let stage = place Link(x: x, y: y)
}
"#,
        )
        .expect("hierarchical source cell must compile into driver metadata");
    let metadata = output
        .metadata()
        .expect("successful elaboration must expose hardware metadata");

    let summary = metadata
        .cell_summaries()
        .iter()
        .find(|summary| summary.callable() == "Link" && summary.instance() == "stage")
        .expect("placed source cell must export an instantiation summary");

    assert!(summary.drives().iter().any(|place| {
        matches!(place, HardwarePlace::Ident(name) if name == "y")
            || matches!(place, HardwarePlace::Object { name, .. } if name == "y")
    }));
    assert!(summary.reads().iter().any(|place| {
        matches!(place, HardwarePlace::Ident(name) if name == "x")
            || matches!(place, HardwarePlace::Object { name, .. } if name == "x")
    }));
    assert!(summary.creates().is_empty());
    assert!(summary.origin().expansion_stack().is_empty());
}

#[test]
fn external_summary_registry_resolves_missing_boundary() {
    let boundary_origin = Span::new_in(SourceId::new(12), 10, 20);
    let declaration_origin = Span::new_in(SourceId::new(13), 30, 40);
    let mut declaration =
        CellSummaryDeclaration::exact_at_span("VendorCell", "u_vendor", declaration_origin);
    declaration.add_drive(SummaryPlace::Ident("u_vendor.out".to_string()));
    declaration.add_read(SummaryPlace::Ident("u_vendor.in".to_string()));
    declaration.add_create("u_vendor_state");

    let registry = CellSummaryRegistry::from_iter([declaration]);
    let boundary = CellBoundarySummary::missing_at_span("VendorCell", "u_vendor", boundary_origin);
    let resolved = boundary.resolve_with(&registry);

    let summary = resolved
        .available_summary()
        .expect("registered summary must materialize as an available cell summary");

    assert_eq!(summary.callable(), "VendorCell");
    assert_eq!(summary.instance(), "u_vendor");
    assert_eq!(summary.origin().source(), SourceId::new(13));
    assert_eq!(summary.origin().span_start(), 30);
    assert_eq!(summary.origin().span_end(), 40);
    assert!(summary.origin().labels().is_empty());
    assert!(
        summary
            .drives()
            .iter()
            .any(|place| matches!(place, SummaryPlace::Ident(name) if name == "u_vendor.out"))
    );
    assert!(
        summary
            .reads()
            .iter()
            .any(|place| matches!(place, SummaryPlace::Ident(name) if name == "u_vendor.in"))
    );
    assert!(
        summary
            .creates()
            .iter()
            .any(|name| name == "u_vendor_state")
    );
}
