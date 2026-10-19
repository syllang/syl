use super::ModulePath;
use crate::{DocumentUri, ProjectResolver, SourceDocument};
use syl_span::SourceId;

#[test]
fn module_path_value_object_validates_fallback_identity() {
    let resolver = ProjectResolver::new();
    let document = SourceDocument::from_overlay(
        DocumentUri::new("untitled:syl/controller"),
        "cell Controller(out Bit) {}".to_string(),
        crate::DocumentVersion::new(1),
        None,
    );

    let (path, diagnostics) = ModulePath::for_document(&resolver, &document, SourceId::new(0));

    assert_eq!(path.parts, vec!["controller"]);
    assert!(diagnostics.is_empty());
}

#[test]
fn module_path_value_object_reports_invalid_fallback_segment() {
    let resolver = ProjectResolver::new();
    let document = SourceDocument::from_overlay(
        DocumentUri::new("untitled:syl/controller-v2"),
        "cell Controller(out Bit) {}".to_string(),
        crate::DocumentVersion::new(1),
        None,
    );

    let (path, diagnostics) = ModulePath::for_document(&resolver, &document, SourceId::new(0));

    assert_eq!(path.parts, vec!["controller-v2"]);
    assert_eq!(diagnostics.len(), 1);
    assert_eq!(
        diagnostics[0].code.as_deref(),
        Some("E_INVALID_MODULE_PATH")
    );
}
