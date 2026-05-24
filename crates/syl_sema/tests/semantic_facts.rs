use syl_hir::{DefId, ExprId, HirDesign, LocalId};
use syl_sema::const_eval::ConstValue;
use syl_sema::{
    CapabilityKind, ConstFactKey, HirFactId, Layout, LoweringError, ProtocolFieldDirection,
    SemanticCompiler, SemanticResolution, TirError, WordEncoding,
};
use syl_span::{SourceId, Span};
use syl_syntax::{AstFile, SourceParser};

#[test]
fn semantic_facts_bundle_exposes_queryable_phase3_tables() {
    let shared = r#"
package shared;

const WIDTH: Nat = 4 + 1

interface Stream<D: Domain> {
    payload: UInt<WIDTH>
    valid: Bit
    ready: Bit

    view sink {
        in payload
        in valid
        out ready
    }
}
"#;
    let app = r#"
package app;

use shared.Stream;
use shared.WIDTH;

module Top<D: Domain>(
    clk: in Clock<D>,
    rst: in Reset<D>,
    up: in Stream<D>.sink,
    y: out UInt<WIDTH>,
) {
    y := up.payload
}
"#;
    let files = parse_sources(&[shared, app]);
    let compiler = SemanticCompiler::new();
    let session = compiler.session(&files);
    let hir = session
        .resolve_hir()
        .expect("phase3 facts fixture must resolve HIR");
    let output = session.check();
    let tir = output
        .tir()
        .expect("phase3 facts fixture must type-check into TIR");
    let facts = output
        .facts()
        .expect("type-checked semantic output must expose facts");
    let hir_design = tir.design().hir();

    let stream_def = def_id(hir_design, "Stream");
    let width_def = def_id(hir_design, "WIDTH");
    let top_def = def_id(hir_design, "Top");
    let domain_local = local_id(hir_design, top_def, "D");
    let clk_local = local_id(hir_design, top_def, "clk");
    let rst_local = local_id(hir_design, top_def, "rst");
    let up_local = local_id(hir_design, top_def, "up");
    let y_local = local_id(hir_design, top_def, "y");
    let up_expr = expr_id_at(app, SourceId::new(1), "up.payload", 0, 2, hir_design);

    let resolution = hir.resolution();
    let app_package = resolution
        .graph()
        .packages()
        .iter()
        .find(|package| package.path().display() == "app")
        .expect("app package must appear in the resolution graph");
    assert_eq!(app_package.imports().len(), 2);
    assert!(
        app_package
            .imports()
            .iter()
            .any(|edge| edge.target() == Some(stream_def))
    );
    assert!(
        app_package
            .imports()
            .iter()
            .any(|edge| edge.target() == Some(width_def))
    );
    assert_eq!(
        resolution.get(HirFactId::Expr(up_expr)),
        Some(SemanticResolution::Local(up_local))
    );

    let domain_ty = facts
        .types()
        .get(HirFactId::Local(domain_local))
        .expect("domain generic must have a canonical type id");
    let clk_ty = facts
        .types()
        .get(HirFactId::Local(clk_local))
        .expect("clock param must have a canonical type id");
    let rst_ty = facts
        .types()
        .get(HirFactId::Local(rst_local))
        .expect("reset param must have a canonical type id");
    let up_ty = facts
        .types()
        .get(HirFactId::Local(up_local))
        .expect("view param must have a canonical type id");
    let y_ty = facts
        .types()
        .get(HirFactId::Local(y_local))
        .expect("output param must have a canonical type id");

    assert_ne!(clk_ty, rst_ty);
    assert_eq!(
        facts.consts().value(HirFactId::Def(width_def)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        facts.consts().cache_value(ConstFactKey::Def(width_def)),
        Some(ConstValue::Nat(5))
    );

    let domain_cap = facts
        .capabilities()
        .get(HirFactId::Local(domain_local))
        .expect("domain generic must have capability facts");
    assert!(matches!(domain_cap.kind(), CapabilityKind::Domain));
    let clk_cap = facts
        .capabilities()
        .get(HirFactId::Local(clk_local))
        .expect("clock param must have capability facts");
    assert!(matches!(
        clk_cap.kind(),
        CapabilityKind::Clock {
            domain: Some(id)
        } if *id == domain_ty
    ));
    let rst_cap = facts
        .capabilities()
        .get(HirFactId::Local(rst_local))
        .expect("reset param must have capability facts");
    assert!(matches!(
        rst_cap.kind(),
        CapabilityKind::Reset {
            domain: Some(id)
        } if *id == domain_ty
    ));
    let up_cap = facts
        .capabilities()
        .get(HirFactId::Local(up_local))
        .expect("view param must expose readable and writable fields");
    let CapabilityKind::View(view_caps) = up_cap.kind() else {
        panic!("expected view capability facts, got {:?}", up_cap.kind());
    };
    assert_eq!(view_caps.interface(), stream_def);
    assert_eq!(view_caps.view(), "sink");
    assert_eq!(
        view_caps.readable_fields(),
        &["payload".to_string(), "valid".to_string()]
    );
    assert_eq!(view_caps.writable_fields(), &["ready".to_string()]);

    let up_layout = facts
        .layouts()
        .get(up_ty)
        .expect("view type must have layout facts");
    assert!(matches!(
        up_layout,
        Layout::View {
            interface,
            view,
            fields
        } if *interface == stream_def && view == "sink" && fields == &vec!["payload".to_string(), "valid".to_string(), "ready".to_string()]
    ));
    let y_layout = facts
        .layouts()
        .get(y_ty)
        .expect("word type must have layout facts");
    assert!(matches!(
        y_layout,
        Layout::Word {
            encoding: WordEncoding::UInt,
            ..
        }
    ));

    let protocol = facts
        .protocols()
        .get(stream_def)
        .expect("interface protocol summary must be recorded");
    assert_eq!(protocol.name(), "Stream");
    assert_eq!(
        protocol.fields(),
        &[
            "payload".to_string(),
            "valid".to_string(),
            "ready".to_string(),
        ]
    );
    assert_eq!(protocol.views().len(), 1);
    assert_eq!(protocol.views()[0].name(), "sink");
    assert_eq!(protocol.views()[0].fields().len(), 3);
    assert!(matches!(
        protocol.views()[0].fields()[0].direction(),
        ProtocolFieldDirection::In
    ));
}

#[test]
fn semantic_errors_expose_structured_variants() {
    let file = SourceParser::new(
        r#"
module Bad(x: in Missing) {
}
"#,
    )
    .parse_file()
    .expect("structured error fixture must parse");
    let files = [file];
    let session = SemanticCompiler::new().session(&files);
    let hir = session
        .resolve_hir()
        .expect("HIR must resolve before type error");
    let err = hir
        .check_tir()
        .expect_err("unknown types must fail during sema");

    match err.kind() {
        LoweringError::Tir(TirError::UnknownType { name }) => assert_eq!(name, "Missing"),
        other => panic!("expected TirError::UnknownType, got {other:?}"),
    }
}

#[test]
fn const_facts_are_deterministic_across_repeated_runs() {
    let file = SourceParser::new(
        r#"
const WIDTH: Nat = 2 + 3

module Top(y: out UInt<WIDTH>) {
}
"#,
    )
    .parse_file()
    .expect("const determinism fixture must parse");
    let files = [file];
    let compiler = SemanticCompiler::new();

    let first = compiler.session(&files).check();
    let second = compiler.session(&files).check();

    assert_eq!(
        first.facts().expect("first run must expose facts").consts(),
        second
            .facts()
            .expect("second run must expose facts")
            .consts()
    );
}

fn parse_sources(sources: &[&str]) -> Vec<AstFile> {
    sources
        .iter()
        .enumerate()
        .map(|(index, source)| {
            SourceParser::new_in(source, SourceId::new(index))
                .parse_file()
                .unwrap_or_else(|errors| {
                    panic!("fixture {index} must parse:\n{}", diagnostics_text(&errors))
                })
        })
        .collect()
}

fn def_id(hir: &HirDesign, name: &str) -> DefId {
    hir.defs
        .iter()
        .find(|def| def.name == name)
        .unwrap_or_else(|| panic!("missing definition {name}"))
        .id
}

fn local_id(hir: &HirDesign, owner: DefId, name: &str) -> LocalId {
    hir.locals
        .iter()
        .find(|local| local.owner == owner && local.name == name)
        .unwrap_or_else(|| panic!("missing local {name} in owner {}", owner.get()))
        .id
}

fn expr_id_at(
    source: &str,
    source_id: SourceId,
    needle: &str,
    start_offset: usize,
    width: usize,
    hir: &HirDesign,
) -> ExprId {
    let base = source
        .find(needle)
        .unwrap_or_else(|| panic!("missing needle {needle}"));
    let start = base + start_offset;
    let span = Span::new_in(source_id, start, start + width);
    hir.exprs
        .iter()
        .find(|expr| expr.span == span)
        .unwrap_or_else(|| panic!("missing expr at {}..{}", span.start, span.end))
        .id
}

fn diagnostics_text<T: ToString>(diagnostics: &[T]) -> String {
    diagnostics
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("\n")
}
