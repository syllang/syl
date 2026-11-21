use super::*;
use syl_span::{SourceId, Span};

#[test]
fn resolves_boundary_with_registered_declaration() {
    let summary_origin = HwOrigin::new(SourceId::new(7), 100, 120, Vec::new());
    let boundary_origin = HwOrigin::new(SourceId::new(8), 200, 220, Vec::new());
    let mut declaration =
        CellSummaryDeclaration::exact("VendorCell", "u_vendor", summary_origin.clone());
    declaration.add_drive(HwPlace::Ident("u_vendor.out".to_string()));
    declaration.add_read(HwPlace::Ident("u_vendor.in".to_string()));
    declaration.add_create("u_vendor_state");

    let registry = CellSummaryRegistry::from_iter([declaration]);
    let boundary = CellBoundarySummary::missing("VendorCell", "u_vendor", boundary_origin);

    let resolved = boundary.resolve_with(&registry);
    let summary = resolved
        .available_summary()
        .expect("registered summary must resolve boundary");

    assert_eq!(resolved.status(), CellSummaryStatus::Available);
    assert_eq!(summary.callable(), "VendorCell");
    assert_eq!(summary.instance(), "u_vendor");
    assert_eq!(summary.origin(), &summary_origin);
    assert_eq!(
        summary.drives(),
        &[HwPlace::Ident("u_vendor.out".to_string())]
    );
    assert_eq!(
        summary.reads(),
        &[HwPlace::Ident("u_vendor.in".to_string())]
    );
    assert_eq!(summary.creates(), &["u_vendor_state".to_string()]);
}

#[test]
fn leaves_boundary_missing_without_matching_summary() {
    let mut declaration = CellSummaryDeclaration::exact(
        "VendorCell",
        "u_other",
        HwOrigin::new(SourceId::new(9), 300, 320, Vec::new()),
    );
    declaration.add_drive(HwPlace::Ident("u_other.out".to_string()));
    let registry = CellSummaryRegistry::from_iter([declaration]);
    let boundary = CellBoundarySummary::missing(
        "VendorCell",
        "u_vendor",
        HwOrigin::new(SourceId::new(10), 330, 340, Vec::new()),
    );

    let resolved = boundary.resolve_with(&registry);

    assert_eq!(resolved.status(), CellSummaryStatus::Missing);
    assert!(resolved.available_summary().is_none());
    assert_eq!(
        resolved.require_available().unwrap_err().diagnostic().span,
        Span::new_in(SourceId::new(10), 330, 340)
    );
}
