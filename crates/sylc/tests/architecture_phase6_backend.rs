mod support;

use std::{
    env, fs,
    io::ErrorKind,
    path::{Path, PathBuf},
    process::Command,
};

use support::MiddleCompiler;
use syl_emit::{CompileError, SystemVerilogBackend};
use syl_hw::{
    HwDirection, HwExpr, HwItem, HwOrigin, HwPort, ParametricHwDesign, ParametricHwItem,
    ParametricHwModule,
};
use syl_span::SourceId;

#[test]
fn architecture_phase6_hw_validation_happens_before_sv_emission() {
    let invalid = ParametricHwDesign::new(vec![
        module(
            "Top",
            vec![port(HwDirection::Out, "1", "y")],
            vec![assign("y", "y")],
        ),
        module("Top", Vec::new(), Vec::new()),
    ]);

    let err = SystemVerilogBackend::new()
        .emit(&invalid)
        .expect_err("duplicate HW module names must fail before SystemVerilog lowering");

    assert!(matches!(
        err,
        CompileError::InvalidHwir { ref report }
            if matches!(
                report.diagnostics(),
                [syl_hw::HwValidationDiagnostic::DuplicateModule { name }, ..] if name == "Top"
            )
    ));
}

#[test]
fn architecture_phase6_emits_inout_and_high_z() {
    let design = ParametricHwDesign::new(vec![module(
        "PadTop",
        vec![port(HwDirection::InOut, "1", "pad")],
        vec![ParametricHwItem::core(
            HwItem::ContinuousDrive {
                lhs: HwExpr::Ident("pad".to_string()),
                rhs: HwExpr::HighZ,
            },
            origin(),
        )],
    )]);

    let sv = SystemVerilogBackend::new()
        .emit(&design)
        .expect("inout high-z design must emit");

    assert!(sv.contains("inout pad"));
    assert!(sv.contains("assign pad = 'z;"));
}

#[test]
fn architecture_phase6_emitter_stays_frontend_free_and_hw_checks_stay_outside() {
    let emit_manifest = read_text(&workspace_root().join("crates/syl_emit/Cargo.toml"));
    for forbidden in ["syl_hir", "syl_sema", "syl_elab"] {
        assert!(
            !emit_manifest.contains(forbidden),
            "syl_emit must stay frontend/elaboration free: found {forbidden}"
        );
    }

    let emit_check = read_text(&workspace_root().join("crates/syl_emit/src/check.rs"));
    for forbidden in [
        "UnknownReference",
        "UnknownInstanceTarget",
        "DuplicateBinding",
        "DuplicateModule",
    ] {
        assert!(
            !emit_check.contains(forbidden),
            "backend-independent HW checks drifted back into syl_emit/src/check.rs: {forbidden}"
        );
    }

    let hw_validate_api = read_text(&workspace_root().join("crates/syl_hw/src/validate.rs"));
    let hw_validate_diagnostics =
        read_text(&workspace_root().join("crates/syl_hw/src/validate/diagnostic.rs"));
    let hw_validate_impl =
        read_text(&workspace_root().join("crates/syl_hw/src/validate/validator.rs"));
    for required in ["pub struct HwValidator", "pub struct HwNormalizer"] {
        assert!(
            hw_validate_api.contains(required),
            "syl_hw must own Phase 6 backend-independent validation: missing {required}"
        );
    }
    for required in [
        "UnknownReference",
        "UnknownInstanceTarget",
        "DuplicateBinding",
    ] {
        assert!(
            hw_validate_diagnostics.contains(required) || hw_validate_impl.contains(required),
            "syl_hw must own Phase 6 backend-independent validation: missing {required}"
        );
    }
}

#[test]
fn architecture_phase6_same_hwir_produces_dump_and_sv() {
    let hwir = MiddleCompiler::new()
        .compile_sources(&[inline_passthrough_source()])
        .expect("Phase 6 fixture must lower to HW IR once");

    let hwir_dump = hwir.debug_dump();
    let sv_dump = SystemVerilogBackend::new()
        .debug_dump(&hwir)
        .expect("the same HW IR should lower into a backend debug view");
    let sv = SystemVerilogBackend::new()
        .emit(&hwir)
        .expect("the same HW IR should emit SystemVerilog text");

    assert!(hwir_dump.contains("hwir modules=2 [Child, Top]"));
    assert!(sv_dump.contains("sv_ast modules=2 [Child, Top]"));
    assert!(sv.contains("module Child"));
    assert!(sv.contains("module Top"));
}

#[test]
fn architecture_phase6_golden_sv_output_stays_stable() {
    let hwir = MiddleCompiler::new()
        .compile_sources(&[inline_passthrough_source()])
        .expect("golden fixture must lower to HW IR");
    let actual = SystemVerilogBackend::new()
        .emit(&hwir)
        .expect("golden fixture must emit SystemVerilog");
    let expected = include_str!("golden/phase6_child_top.sv");

    assert_eq!(
        actual.as_str(),
        expected,
        "golden SV drifted; update the fixture only for intentional backend formatting changes"
    );
}

#[test]
fn architecture_phase6_verilator_smoke_covers_example_and_integration_designs() {
    if !verilator_available() {
        eprintln!("Skipping Verilator smoke because `verilator` is not available");
        return;
    }

    let backend = SystemVerilogBackend::new();
    let example_sv = backend
        .emit(
            &MiddleCompiler::new()
                .compile_sources(&[include_str!("../../../examples/minimal_features.syl")])
                .expect("example fixture must lower to HW IR"),
        )
        .expect("example fixture must emit SystemVerilog");
    let integration_sv = backend
        .emit(
            &MiddleCompiler::new()
                .compile_sources(&[inline_passthrough_source()])
                .expect("integration fixture must lower to HW IR"),
        )
        .expect("integration fixture must emit SystemVerilog");

    let example_path = temp_sv_path("phase6-example");
    let integration_path = temp_sv_path("phase6-integration");
    fs::write(&example_path, example_sv).expect("example smoke file must be writable");
    fs::write(&integration_path, integration_sv).expect("integration smoke file must be writable");

    let example_result = verilator_lint(&example_path, None);
    let integration_result = verilator_lint(&integration_path, Some("Top"));

    let _ = fs::remove_file(&example_path);
    let _ = fs::remove_file(&integration_path);

    assert!(
        example_result.status.success(),
        "Verilator smoke failed for example fixture\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&example_result.stdout),
        String::from_utf8_lossy(&example_result.stderr)
    );
    assert!(
        integration_result.status.success(),
        "Verilator smoke failed for integration fixture\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&integration_result.stdout),
        String::from_utf8_lossy(&integration_result.stderr)
    );
}

fn inline_passthrough_source() -> &'static str {
    r#"
module Child(x: in Bit, y: out Bit) {
    y := x
}

module Top(x: in Bit, y: out Bit) {
    signal tmp: Bit
    inst child = Child(x: x, y: tmp)
    y := tmp
}
"#
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("tests must locate the workspace root")
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}

fn origin() -> HwOrigin {
    HwOrigin::new(SourceId::new(0), 0, 0, Vec::new())
}

fn module(name: &str, ports: Vec<HwPort>, items: Vec<ParametricHwItem>) -> ParametricHwModule {
    ParametricHwModule::new(name, Vec::new(), ports, items)
}

fn port(direction: HwDirection, width: &str, name: &str) -> HwPort {
    HwPort::new(direction, width, name)
}

fn assign(lhs: &str, rhs: &str) -> ParametricHwItem {
    ParametricHwItem::core(
        HwItem::ContinuousDrive {
            lhs: HwExpr::Ident(lhs.to_string()),
            rhs: HwExpr::Ident(rhs.to_string()),
        },
        origin(),
    )
}

fn temp_sv_path(stem: &str) -> PathBuf {
    env::temp_dir().join(format!(
        "{stem}-{}-{}.sv",
        std::process::id(),
        unique_suffix()
    ))
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock must be after the unix epoch")
        .as_nanos()
}

fn verilator_available() -> bool {
    match Command::new("verilator").arg("--version").output() {
        Ok(output) => output.status.success(),
        Err(err) if err.kind() == ErrorKind::NotFound => false,
        Err(err) => panic!("failed to probe verilator availability: {err}"),
    }
}

fn verilator_lint(path: &Path, top: Option<&str>) -> std::process::Output {
    let mut command = Command::new("verilator");
    command.arg("--lint-only").arg("--sv");
    if let Some(top) = top {
        command.arg("--top-module").arg(top);
    }
    command.arg(path);
    command
        .output()
        .unwrap_or_else(|err| panic!("failed to execute verilator for {}: {err}", path.display()))
}
