mod support;

use support::MiddleCompiler;
use syl_hw::{HwPlace as HardwarePlace, ParametricHwDesign};
use syl_sema::cell_summary::{
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

    fn compile_hwir(&self, source: &str) -> Result<ParametricHwDesign, String> {
        let file = SourceParser::new(source).parse_file().map_err(|errs| {
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        self.middle
            .compile_files(&[file])
            .map_err(|err| err.to_string())
    }
}

#[test]
fn inline_cell_expansion_exports_public_driver_summary() {
    let hwir = CellSummaryHarness::new()
        .compile_hwir(
            r#"
cell MakePair() -> y: Bit {
    signal tmp: Bit := 1
    y := tmp
}

module Top(y: out Bit) {
    alias made = MakePair()
    y := made
}
"#,
        )
        .expect("cell expansion must compile into parent module");

    let summary = hwir
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
