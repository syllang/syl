use std::process::Command;

#[test]
fn cli_help_flag_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_sylc"))
        .arg("--help")
        .output()
        .expect("test must execute sylc --help");

    assert!(
        output.status.success(),
        "sylc --help failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--std-root"));
    assert!(stdout.contains("--out"));
}

#[test]
fn cli_version_flag_prints_crate_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_sylc"))
        .arg("--version")
        .output()
        .expect("test must execute sylc --version");

    assert!(
        output.status.success(),
        "sylc --version failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")));
    assert!(stdout.contains("sylc"));
}

use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Output,
    sync::atomic::{AtomicU64, Ordering},
};

struct TestProject(PathBuf);
impl TestProject {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "syl-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self(path.canonicalize().unwrap())
    }
    fn write(&self, path: &str, text: &str) -> PathBuf {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
        path
    }
    fn run(&self, cwd: &str, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_sylc"))
            .current_dir(self.0.join(cwd))
            .args(args)
            .output()
            .unwrap()
    }
}
impl Drop for TestProject {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

fn success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
fn failure(output: &Output) {
    assert_eq!(
        output.status.code(),
        Some(1),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stdout.is_empty());
}
fn report(output: &Output) -> Value {
    serde_json::from_slice(&output.stderr).expect("stderr must contain exactly one JSON report")
}

const CLEAN: &str = "cell Top(a: in Bit, y: out Bit) { y := a }\n";
const DUPLICATE: &str = "cell Top(y: out Bit) {\n    y := 0\n    y := 1\n}\n";

#[test]
fn check_discovers_manifest_from_nested_cwd_and_resolves_all_root_types() {
    let project = TestProject::new();
    project.write("syl.toml", "[paths]\nworkspace_roots = ['src']\nstd_roots = ['stdlib']\npackage_roots = ['packages']\n");
    project.write("src/local.syl", "bundle Local { data: Bit }\n");
    project.write("stdlib/logic.syl", "bundle Flag { data: Bit }\n");
    project.write("packages/pkg.syl", "bundle Item { data: Bit }\n");
    project.write("src/top.syl", "use local.Local\nuse pkg.Item\nuse std.logic.Flag\ncell Top(a: in Item, b: in Flag, c: in Local, y: out Bit) { y := a.data xor b.data xor c.data }\n");
    fs::create_dir_all(project.0.join("src/nested")).unwrap();
    let output = project.run("src/nested", &["check"]);
    success(&output);
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
    let output = project.run("src/nested", &["check", "--diagnostics-format", "json"]);
    success(&output);
    assert!(output.stdout.is_empty());
    assert_eq!(
        report(&output),
        json!({"schema_version":1,"diagnostics":[]})
    );
}

#[test]
fn explicit_emit_and_legacy_emit_produce_the_same_systemverilog() {
    let project = TestProject::new();
    project.write("syl.toml", "[paths]\nworkspace_roots = ['src']");
    project.write("src/top.syl", CLEAN);
    let output = project.run(
        "",
        &[
            "emit",
            "--manifest",
            "syl.toml",
            "--out",
            "out.sv",
            "--diagnostics-format",
            "json",
        ],
    );
    success(&output);
    assert!(output.stdout.is_empty());
    assert_eq!(report(&output)["diagnostics"], json!([]));
    let verilog = fs::read_to_string(project.0.join("out.sv")).unwrap();
    assert!(verilog.contains("module Top"));
    assert!(verilog.contains("endmodule"));
    let legacy = project.run("", &["src/top.syl"]);
    success(&legacy);
    assert_eq!(legacy.stdout, verilog.as_bytes());
    assert!(legacy.stderr.is_empty());
    let json_emit = project.run("", &["--diagnostics-format", "json", "emit", "src/top.syl"]);
    success(&json_emit);
    assert_eq!(json_emit.stdout, verilog.as_bytes());
    assert_eq!(report(&json_emit)["diagnostics"], json!([]));
}

#[test]
fn cli_package_roots_work_without_a_manifest_and_override_manifest_roots() {
    let project = TestProject::new();
    project.write(
        "app/top.syl",
        "use pkg.Item\ncell Top(a: in Item, y: out Bit) { y := a.data }\n",
    );
    project.write("packages/pkg.syl", "bundle Item { data: Bit }\n");
    let no_manifest = project.run("app", &["--package-root", "../packages", "top.syl"]);
    success(&no_manifest);
    assert!(String::from_utf8_lossy(&no_manifest.stdout).contains("module Top"));
    project.write("app/syl.toml", "[paths]\npackage_roots = ['../wrong']");
    project.write("wrong/pkg.syl", "bundle Item { different: Bit }\n");
    failure(&project.run("app", &["check", "top.syl"]));
    success(&project.run(
        "app",
        &["check", "--package-root", "../packages", "top.syl"],
    ));
}

#[test]
fn cli_std_roots_override_manifest_roots_and_preserve_order_across_subcommand() {
    let project = TestProject::new();
    project.write("app/syl.toml", "[paths]\nstd_roots = ['../wrong']");
    project.write(
        "app/top.syl",
        "use std.logic.Item\ncell Top(a: in Item, y: out Bit) { y := a.data }\n",
    );
    project.write("correct/logic.syl", "bundle Item { data: Bit }\n");
    project.write("wrong/logic.syl", "bundle Item { different: Bit }\n");
    failure(&project.run("app", &["check"]));
    let output = project.run(
        "app",
        &[
            "--std-root",
            "../correct",
            "check",
            "--std-root",
            "../wrong",
        ],
    );
    success(&output);
    assert!(output.stdout.is_empty());
}

#[test]
fn trusted_summary_from_manifest_drives_human_and_json_drc() {
    let project = TestProject::new();
    project.write(
        "syl.toml",
        "[paths]\nworkspace_roots = ['src']\nopaque_summaries = ['vendor.json']",
    );
    project.write("vendor.json", r#"{"schema_version":1,"summaries":[{"kind":"precompiled_cell","callable":"VendorLatch","driven_fields":["y"],"trust_boundary":"trusted_precompiled"}]}"#);
    project.write("src/top.syl", "extern cell VendorLatch(y: in Bit)\ncell Top(y: out Bit) {\n signal tmp: Bit := 0\n let vendor = place VendorLatch(y: tmp)\n y := tmp\n}\n");
    let output = project.run("", &["check"]);
    failure(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("E_MIDDLE_DUPLICATE_HARDWARE_DRIVER"));
    assert!(stderr.contains("src/top.syl:"));
    assert!(stderr.contains("signal tmp: Bit := 0"));
    assert!(stderr.contains('^'));
    let output = project.run("", &["check", "--diagnostics-format", "json"]);
    failure(&output);
    assert!(
        report(&output)["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["code"] == "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER" && !d["location"].is_null())
    );
}

#[test]
fn human_parse_and_drc_diagnostics_include_source_and_related_notes() {
    let project = TestProject::new();
    project.write("top.syl", "cell Top(a: in Bit {\n");
    let output = project.run("", &["check", "--diagnostics-format", "human", "top.syl"]);
    failure(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("top.syl:1:"));
    assert!(stderr.contains("cell Top(a: in Bit {"));
    assert!(stderr.contains('^'));
    project.write("top.syl", DUPLICATE);
    let output = project.run("", &["check", "top.syl"]);
    failure(&output);
    let stderr = String::from_utf8_lossy(&output.stderr);
    for expected in [
        "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER",
        "top.syl:3:5",
        "y := 1",
        "^",
        "note:",
        "~",
    ] {
        assert!(stderr.contains(expected), "missing {expected}: {stderr}");
    }
}

#[test]
fn json_success_diagnostics_and_project_errors_are_single_documents() {
    let project = TestProject::new();
    project.write("top.syl", CLEAN);
    let output = project.run("", &["check", "top.syl", "--diagnostics-format", "json"]);
    success(&output);
    assert!(output.stdout.is_empty());
    assert_eq!(
        report(&output),
        json!({"schema_version":1,"diagnostics":[]})
    );
    project.write("top.syl", DUPLICATE);
    let output = project.run("", &["--diagnostics-format", "json", "check", "top.syl"]);
    failure(&output);
    let report = report(&output);
    let diagnostic = report["diagnostics"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["code"] == "E_MIDDLE_DUPLICATE_HARDWARE_DRIVER")
        .unwrap();
    assert_eq!(diagnostic["location"]["file"], "top.syl");
    assert_eq!(diagnostic["location"]["start_line"], 3);
    assert_eq!(diagnostic["location"]["start_column"], 5);
    assert!(!diagnostic["related"].as_array().unwrap().is_empty());
    for (manifest, summary, code) in [
        ("[", None, "E_MANIFEST_PARSE"),
        ("[project]\nname = ' '", None, "E_MANIFEST_INVALID"),
        (
            "[paths]\nopaque_summaries = ['s.json']",
            Some("{"),
            "E_OPAQUE_SUMMARY_PARSE",
        ),
        (
            "[paths]\nopaque_summaries = ['s.json']",
            Some("{\"schema_version\":2}"),
            "E_OPAQUE_SUMMARY_INVALID",
        ),
        (
            "[paths]\nopaque_summaries = ['missing.json']",
            None,
            "E_PROJECT",
        ),
    ] {
        project.write("syl.toml", manifest);
        if let Some(summary) = summary {
            project.write("s.json", summary);
        }
        let output = project.run("", &["check", "--diagnostics-format", "json"]);
        failure(&output);
        let parsed: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(parsed["diagnostics"].as_array().unwrap().len(), 1);
        assert_eq!(parsed["diagnostics"][0]["code"], code);
        assert_eq!(parsed["diagnostics"][0]["source"], "syl_session");
        assert!(parsed["diagnostics"][0]["location"].is_null());
    }
}

#[test]
fn discovery_detects_conflicts_and_explicit_manifest_bypasses_discovery() {
    let project = TestProject::new();
    project.write("a/syl.toml", "");
    project.write("a/first.syl", CLEAN);
    project.write("b/syl.toml", "");
    project.write("b/second.syl", "cell Other(y: out Bit) { y := 0 }\n");
    let output = project.run("", &["check", "a/first.syl", "b/second.syl"]);
    failure(&output);
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("inputs span multiple syl.toml manifests")
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("pass --manifest explicitly"));
    project.write("b/syl.toml", "invalid manifest");
    success(&project.run(
        "",
        &[
            "check",
            "--manifest",
            "a/syl.toml",
            "a/first.syl",
            "b/second.syl",
        ],
    ));
    failure(&project.run("", &["check", "--manifest", "missing.toml", "a/first.syl"]));
    // An input without a manifest can accompany an input with a unique manifest.
    project.write("other.syl", "cell Other(y: out Bit) { y := 0 }\n");
    success(&project.run("", &["check", "other.syl", "a/first.syl"]));
}

#[test]
fn missing_inputs_empty_roots_and_output_errors_are_clear() {
    let project = TestProject::new();
    let output = project.run("", &["check"]);
    failure(&output);
    assert!(String::from_utf8_lossy(&output.stderr).contains("no input files specified"));
    project.write("syl.toml", "[paths]\nworkspace_roots = []");
    failure(&project.run("", &["check"]));
    project.write("top.syl", CLEAN);
    success(&project.run("", &["check", "top.syl"]));
    let output = project.run(
        "",
        &[
            "emit",
            "top.syl",
            "--out",
            ".",
            "--diagnostics-format",
            "json",
        ],
    );
    failure(&output);
    assert_eq!(report(&output)["diagnostics"][0]["code"], "E_IO");
    assert!(!Path::new(&project.0.join("out.sv")).exists());
}
