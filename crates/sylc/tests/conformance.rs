use std::{
    fs,
    path::{Path, PathBuf},
};

use syl_elab::HardwareCompiler;
use syl_emit::{CompileError as EmitError, SystemVerilogBackend};
use syl_hw::{HwValidationDiagnostic, ParametricHwDesign, ParametricHwModule};
use syl_sema::SemanticCompiler;
use syl_session::{AnalysisHost, ProjectConfig};
use syl_span::{Diagnostic, SourceId};
use syl_syntax::{AstFile, SourceParser};

#[test]
fn conformance_parse_cases_are_partitioned_and_code_stable() {
    for case in syl_cases("conformance/parse/positive") {
        let output = SourceParser::new_in(&read_text(&case), SourceId::new(0)).parse_file_partial();
        assert!(
            output.diagnostics.is_empty(),
            "{} should parse without diagnostics: {:?}",
            case.display(),
            diagnostic_codes(&output.diagnostics)
        );
    }

    for case in syl_cases("conformance/parse/negative") {
        let output = SourceParser::new_in(&read_text(&case), SourceId::new(0)).parse_file_partial();
        assert_expected_codes(&case, &output.diagnostics);
    }
}

#[test]
fn conformance_sema_cases_assert_stable_codes() {
    for case in syl_cases("conformance/sema/positive") {
        let files = parse_case_files(std::slice::from_ref(&case));
        SemanticCompiler::new()
            .session(&files)
            .resolve_hir()
            .and_then(|hir| hir.check_tir())
            .unwrap_or_else(|err| panic!("{} should pass sema: {err}", case.display()));
    }

    for case in syl_cases("conformance/sema/negative") {
        let files = parse_case_files(std::slice::from_ref(&case));
        let err = SemanticCompiler::new()
            .session(&files)
            .resolve_hir()
            .and_then(|hir| hir.check_tir())
            .expect_err("negative sema conformance case should fail");
        assert_expected_codes(&case, &[err.to_diagnostic()]);
    }
}

#[test]
fn conformance_elab_cases_assert_stable_codes() {
    for case in syl_cases("conformance/elab/positive") {
        let files = parse_case_files(std::slice::from_ref(&case));
        let tir = SemanticCompiler::new()
            .session(&files)
            .resolve_hir()
            .and_then(|hir| hir.check_tir())
            .unwrap_or_else(|err| panic!("{} should pass sema: {err}", case.display()));
        let output = HardwareCompiler::new().output_for_tir(&tir);
        assert!(
            output.diagnostics().is_empty(),
            "{} should elaborate without diagnostics: {:?}",
            case.display(),
            diagnostic_codes(output.diagnostics())
        );
    }

    for case in syl_cases("conformance/elab/negative") {
        let files = parse_case_files(std::slice::from_ref(&case));
        let tir = SemanticCompiler::new()
            .session(&files)
            .resolve_hir()
            .and_then(|hir| hir.check_tir())
            .unwrap_or_else(|err| panic!("{} should reach elab: {err}", case.display()));
        let output = HardwareCompiler::new().output_for_tir(&tir);
        assert_expected_codes(&case, output.diagnostics());
    }
}

#[test]
fn conformance_backend_snapshot_and_negative_validation_are_executable() {
    for case in syl_cases("conformance/backend/positive") {
        let verilog = emit_case(&case);
        let expected = read_text(&case.with_extension("sv"));
        assert_eq!(
            verilog,
            expected,
            "{} backend snapshot drifted",
            case.display()
        );
    }

    let err = SystemVerilogBackend::new()
        .emit(&ParametricHwDesign::new(vec![
            ParametricHwModule::new("Dup", Vec::new(), Vec::new(), Vec::new()),
            ParametricHwModule::new("Dup", Vec::new(), Vec::new(), Vec::new()),
        ]))
        .expect_err("duplicate HW modules must fail backend validation");
    let code = match err {
        EmitError::InvalidHwir { report } => match report.diagnostics() {
            [HwValidationDiagnostic::DuplicateModule { .. }, ..] => "E_HW_DUPLICATE_MODULE",
            other => panic!("unexpected backend diagnostics: {other:?}"),
        },
        other => panic!("unexpected backend error: {other:?}"),
    };
    let expected =
        read_text(&workspace_root().join("conformance/backend/negative/duplicate_module.codes"));
    assert_eq!(code, expected.trim());
}

#[test]
fn conformance_examples_and_std_user_remain_compatible() {
    let workspace = workspace_root();
    let mut host = AnalysisHost::with_config(
        ProjectConfig::new()
            .with_workspace_root(workspace.clone())
            .with_std_root(workspace.join("examples")),
    );
    for input in [
        "examples/mvp",
        "examples/pipeline_user.syl",
        "examples/std_user",
    ] {
        let snapshot = host
            .load(&[workspace.join(input)])
            .unwrap_or_else(|err| panic!("{input} should load: {err}"));
        assert!(
            snapshot.diagnostics().is_empty(),
            "{input} parse/import diagnostics: {:?}",
            diagnostic_codes(snapshot.diagnostics())
        );
        let sema = snapshot.semantic_diagnostics();
        assert!(
            sema.is_empty(),
            "{input} semantic diagnostics: {:?}",
            diagnostic_codes(&sema)
        );
        let hwir = snapshot
            .hwir()
            .unwrap_or_else(|| panic!("{input} should produce HWIR"));
        SystemVerilogBackend::new()
            .emit(hwir)
            .unwrap_or_else(|err| panic!("{input} should emit SystemVerilog: {err}"));
    }
}

#[test]
fn conformance_parser_differential_lossless_roundtrip_is_stable() {
    let path = workspace_root().join("examples/pipeline_user.syl");
    let source = read_text(&path);
    let (output, syntax) =
        SourceParser::new_in(&source, SourceId::new(0)).parse_file_with_lossless();

    assert!(
        output.diagnostics.is_empty(),
        "{} should parse cleanly: {:?}",
        path.display(),
        diagnostic_codes(&output.diagnostics)
    );
    assert_eq!(syntax.source_text(), source);
}

fn emit_case(case: &Path) -> String {
    let files = parse_case_files(&[case.to_path_buf()]);
    let tir = SemanticCompiler::new()
        .session(&files)
        .resolve_hir()
        .and_then(|hir| hir.check_tir())
        .unwrap_or_else(|err| panic!("{} should pass sema: {err}", case.display()));
    let hwir = HardwareCompiler::new()
        .compile_tir(&tir)
        .unwrap_or_else(|err| panic!("{} should compile HWIR: {err}", case.display()));
    SystemVerilogBackend::new()
        .emit(&hwir)
        .unwrap_or_else(|err| panic!("{} should emit SystemVerilog: {err}", case.display()))
}

fn parse_case_files(paths: &[PathBuf]) -> Vec<AstFile> {
    paths
        .iter()
        .enumerate()
        .map(|(source_id, path)| {
            SourceParser::new_in(&read_text(path), SourceId::new(source_id))
                .parse_file()
                .unwrap_or_else(|diagnostics| {
                    panic!(
                        "{} should parse: {:?}",
                        path.display(),
                        diagnostic_codes(&diagnostics)
                    )
                })
        })
        .collect()
}

fn assert_expected_codes(case: &Path, diagnostics: &[Diagnostic]) {
    let expected = expected_codes(case);
    let actual = diagnostic_codes(diagnostics);
    assert_eq!(
        actual,
        expected,
        "{} diagnostics must use stable codes",
        case.display()
    );
}

fn expected_codes(case: &Path) -> Vec<String> {
    read_text(&case.with_extension("codes"))
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

fn diagnostic_codes(diagnostics: &[Diagnostic]) -> Vec<String> {
    diagnostics
        .iter()
        .map(|diagnostic| {
            diagnostic
                .code
                .clone()
                .unwrap_or_else(|| "<missing-code>".to_string())
        })
        .collect()
}

fn syl_cases(relative: &str) -> Vec<PathBuf> {
    let dir = workspace_root().join(relative);
    let mut cases = Vec::new();
    for entry in fs::read_dir(&dir)
        .unwrap_or_else(|err| panic!("failed to read conformance dir {}: {err}", dir.display()))
    {
        let path = entry
            .unwrap_or_else(|err| panic!("failed to read conformance entry: {err}"))
            .path();
        if path.extension().and_then(|ext| ext.to_str()) == Some("syl") {
            cases.push(path);
        }
    }
    cases.sort();
    cases
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("test must locate workspace root")
}

fn read_text(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()))
}
