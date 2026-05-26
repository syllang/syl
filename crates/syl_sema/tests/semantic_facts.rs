use syl_hir::{DefId, ExprId, HirDesign, LocalId};
use syl_sema::const_eval::{ConstEvalEnv, ConstValue};
use syl_sema::const_mir::ConstMirBuilder;
use syl_sema::{
    CapabilityKind, ConstEvalError, ConstFactKey, DomainFact, HirFactId, Layout, LoweringError,
    ProtocolFieldDirection, SemanticCompiler, SemanticResolution, SemanticSourceFile, TirError,
    WordEncoding,
};
use syl_span::{SourceId, Span};
use syl_syntax::{AstFile, SourceParser};

#[test]
fn semantic_facts_bundle_exposes_queryable_phase3_tables() {
    let shared = r#"
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

module Direct(
    clk: in Clock<Domain>,
    rst: in Reset<Domain>,
) {
}
"#;
    let files = parse_sources(&[shared, app]);
    let compiler = SemanticCompiler::new();
    let session = compiler.session_sources(vec![
        SemanticSourceFile::new(vec!["shared".to_string()], &files[0]),
        SemanticSourceFile::new(vec!["app".to_string()], &files[1]),
    ]);
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
    let up_expr = expr_id_at(
        ExprLookup::new(app, SourceId::new(1), "up.payload", 0, 2),
        hir_design,
    );
    let direct_def = def_id(hir_design, "Direct");
    let direct_clk_local = local_id(hir_design, direct_def, "clk");
    let direct_rst_local = local_id(hir_design, direct_def, "rst");

    let resolution = hir.resolution();
    let graph = resolution.graph();
    let app_package = graph
        .packages()
        .iter()
        .find(|package| package.path().display() == "app")
        .expect("app package must appear in the resolution graph");
    let app_imports = graph.package_imports(app_package.id());
    let app_modules = graph.package_modules(app_package.id());
    assert!(app_modules.contains(&top_def));
    assert!(app_modules.contains(&direct_def));
    assert!(graph.modules().contains(&top_def));
    assert!(graph.modules().contains(&direct_def));
    assert_eq!(app_imports.len(), 2);
    assert!(
        app_imports
            .iter()
            .filter_map(|import| graph.import(*import))
            .any(|edge| {
                edge.path().display() == "shared.Stream"
                    && graph.import_target(edge.id()) == Some(stream_def)
            })
    );
    assert!(
        app_imports
            .iter()
            .filter_map(|import| graph.import(*import))
            .any(|edge| {
                edge.path().display() == "shared.WIDTH"
                    && graph.import_target(edge.id()) == Some(width_def)
            })
    );
    assert_eq!(
        graph
            .definition_path(top_def)
            .expect("module definition path must exist")
            .canonical_path()
            .display(),
        "app.Top"
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
    assert_eq!(
        facts.consts().value(HirFactId::Expr(expr_id_at(
            ExprLookup::new(shared, SourceId::new(0), "4 + 1", 0, "4 + 1".len()),
            hir_design,
        ))),
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
            domain: DomainFact::Named(id)
        } if *id == domain_ty
    ));
    let rst_cap = facts
        .capabilities()
        .get(HirFactId::Local(rst_local))
        .expect("reset param must have capability facts");
    assert!(matches!(
        rst_cap.kind(),
        CapabilityKind::Reset {
            domain: DomainFact::Named(id)
        } if *id == domain_ty
    ));
    let direct_clk_cap = facts
        .capabilities()
        .get(HirFactId::Local(direct_clk_local))
        .expect("direct builtin-domain clock param must have capability facts");
    assert!(matches!(
        direct_clk_cap.kind(),
        CapabilityKind::Clock {
            domain: DomainFact::BuiltinDomain
        }
    ));
    let direct_rst_cap = facts
        .capabilities()
        .get(HirFactId::Local(direct_rst_local))
        .expect("direct builtin-domain reset param must have capability facts");
    assert!(matches!(
        direct_rst_cap.kind(),
        CapabilityKind::Reset {
            domain: DomainFact::BuiltinDomain
        }
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
    let domain_layout = facts
        .layouts()
        .get(domain_ty)
        .expect("domain type must have layout facts");
    assert!(matches!(domain_layout, Layout::Domain));
    let clk_layout = facts
        .layouts()
        .get(clk_ty)
        .expect("clock type must have layout facts");
    assert!(matches!(clk_layout, Layout::Clock));
    let rst_layout = facts
        .layouts()
        .get(rst_ty)
        .expect("reset type must have layout facts");
    assert!(matches!(rst_layout, Layout::Reset));
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
    let source = r#"
fn add_one(x: Nat) -> Nat {
    return x + 1
}

const WIDTH: Nat = add_one(4)
const HEIGHT: Nat = add_one(4)

module Top(y: out UInt<WIDTH>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("const determinism fixture must parse");
    let files = [file];
    let compiler = SemanticCompiler::new();

    let first = compiler.session(&files).check();
    let second = compiler.session(&files).check();
    let first_facts = first.facts().expect("first run must expose facts");
    let second_facts = second.facts().expect("second run must expose facts");
    let first_hir = first
        .tir()
        .expect("first run must produce TIR")
        .design()
        .hir();
    let width_def = def_id(first_hir, "WIDTH");
    let height_def = def_id(first_hir, "HEIGHT");
    let width_call = expr_id_at(
        ExprLookup::new(
            source,
            SourceId::new(0),
            "add_one(4)",
            0,
            "add_one(4)".len(),
        ),
        first_hir,
    );
    let add_one_body = expr_id_at(
        ExprLookup::new(source, SourceId::new(0), "x + 1", 0, "x + 1".len()),
        first_hir,
    );

    assert_eq!(first_facts.consts(), second_facts.consts());
    assert_eq!(
        first_facts.consts().value(HirFactId::Def(width_def)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        first_facts.consts().value(HirFactId::Def(height_def)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        first_facts.consts().value(HirFactId::Expr(width_call)),
        Some(ConstValue::Nat(5))
    );
    assert_eq!(
        first_facts.consts().value(HirFactId::Expr(add_one_body)),
        Some(ConstValue::Nat(5))
    );
}

#[test]
fn extension_fn_method_lowers_into_const_mir() {
    let source = r#"
enum Op {
    Add,
}

fn rank(this op: Op) -> Nat {
    return 1
}

fn use_rank(op: Op) -> Nat {
    return op.rank()
}

module Top(y: out UInt<1>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("extension fn fixture must parse");
    let files = [file];
    let output = SemanticCompiler::new().session(&files).check();
    let tir = output
        .tir()
        .expect("extension fn fixture must type-check into TIR");
    let hir = tir.design().hir();
    let use_rank = def_id(hir, "use_rank");
    let program = ConstMirBuilder::new(tir.design())
        .build()
        .expect("extension fn call must lower into const MIR");
    let function = program
        .function(use_rank)
        .expect("use_rank const MIR function must exist");

    assert!(!function.is_unsupported());
}

#[test]
fn const_evaluator_reports_structured_step_limit_for_long_running_const_fn() {
    let source = r#"
fn burn_steps(limit: Nat) -> Nat {
    var i: Nat = 0

    while i < limit {
        i = i + 1
    }

    return i
}

const WIDTH: Nat = burn_steps(20000)

module Top(y: out UInt<1>) {
}
"#;
    let file = SourceParser::new_in(source, SourceId::new(0))
        .parse_file()
        .expect("step-limit fixture must parse");
    let files = [file];
    let output = SemanticCompiler::new().session(&files).check();
    let tir = output
        .tir()
        .expect("step-limit fixture must still type-check into TIR");
    let hir = tir.design().hir();
    let width_def = def_id(hir, "WIDTH");
    let width_item = hir
        .consts
        .get(&width_def)
        .expect("WIDTH const item must exist");
    let program = ConstMirBuilder::new(tir.design())
        .build()
        .expect("const MIR program lowering must succeed");
    let expr = ConstMirBuilder::new(tir.design()).lower_const_expr(width_def, &width_item.value);
    let mut evaluator = program.evaluator();
    let err = evaluator
        .expr_value(&expr, &mut ConstEvalEnv::with_owner(Some(width_def)))
        .expect_err("long-running const fn must hit the evaluator step limit");

    match err.kind() {
        LoweringError::Const(ConstEvalError::StepLimitExceeded { limit }) => {
            assert_eq!(*limit, 10_000)
        }
        other => panic!("expected structured step-limit error, got {other:?}"),
    }
    assert_eq!(
        output
            .facts()
            .expect("step-limit fixture must still expose facts")
            .consts()
            .value(HirFactId::Def(width_def)),
        None
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

struct ExprLookup<'a> {
    source: &'a str,
    source_id: SourceId,
    needle: &'a str,
    start_offset: usize,
    width: usize,
}

impl<'a> ExprLookup<'a> {
    fn new(
        source: &'a str,
        source_id: SourceId,
        needle: &'a str,
        start_offset: usize,
        width: usize,
    ) -> Self {
        Self {
            source,
            source_id,
            needle,
            start_offset,
            width,
        }
    }
}

fn expr_id_at(lookup: ExprLookup<'_>, hir: &HirDesign) -> ExprId {
    let base = lookup
        .source
        .find(lookup.needle)
        .unwrap_or_else(|| panic!("missing needle {}", lookup.needle));
    let start = base + lookup.start_offset;
    let span = Span::new_in(lookup.source_id, start, start + lookup.width);
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
