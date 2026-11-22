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

#[test]
fn multi_file_emission_rewrites_expanded_instances_and_preserves_sources() {
    let project = TestProject::new();
    project.write(
        "a.syl",
        "cell Fifo(x: out Bit) { x := 1 }\ncell Left(x: out Bit) { let f = place Fifo(x: x) }\n",
    );
    project.write(
        "b.syl",
        "cell Fifo(z: out Bit) { z := 0 }\ncell Right(z: out Bit) { let f = place Fifo(z: z) }\n",
    );
    project.write("top.syl", "use a.Left\nuse b.Right\ncell Top(x: out Bit, z: out Bit) {\nlet one = inplace Left(x: x)\nlet two = inplace Right(z: z)\n}\n");
    success(&project.run(
        ".",
        &["emit", "top.syl", "--out-dir", "sv", "--source-comments"],
    ));
    let read = |name: &str| fs::read_to_string(project.0.join(name)).unwrap();
    let top = read("sv/Top.sv");
    assert!(top.contains("a__Fifo one_f_inst (.x(x));"));
    assert!(top.contains("b__Fifo two_f_inst (.z(z));"));
    assert!(top.contains("top.syl:3\nmodule Top"));
    assert!(top.contains("a.syl:2 (via Left:one)"));
    assert!(read("sv/a__Fifo.sv").contains("// syl: a.Fifo"));
    let list = read("sv/filelist.f");
    for child in ["a__Fifo.sv", "b__Fifo.sv"] {
        assert!(list.find(child).unwrap() < list.find("Top.sv").unwrap());
    }
    success(&project.run(
        ".",
        &[
            "--out-dir",
            "sv",
            "--filelist",
            "sv/custom.f",
            "b.syl",
            "a.syl",
            "top.syl",
        ],
    ));
    assert!(!read("sv/Top.sv").contains("// source:"));
    assert!(read("sv/Top.sv").contains("a__Fifo one_f_inst"));
    assert!(read("sv/custom.f").lines().any(|line| line == "Top.sv"));
}

#[test]
fn verilator_lints_dependency_ordered_filelist() {
    let project = TestProject::new();
    project.write(
        "top.syl",
        "cell Top(x: out Bit) { let f = place Child(x: x) }\ncell Child(x: out Bit) { x := 0 }\n",
    );
    success(&project.run(".", &["--out-dir", "sv", "--source-comments", "top.syl"]));
    assert_eq!(
        fs::read_to_string(project.0.join("sv/filelist.f")).unwrap(),
        "Child.sv\nTop.sv\n"
    );
    lint_filelist(&project.0.join("sv"), "filelist.f");
}

fn lint_filelist(dir: &Path, list: &str) {
    match Command::new("verilator")
        .current_dir(dir)
        .args(["--lint-only", "--sv", "-f", list])
        .output()
    {
        Ok(output) => success(&output),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("verilator unavailable")
        }
        Err(err) => panic!("{err}"),
    }
}

#[test]
fn custom_filelist_quotes_module_paths_with_spaces() {
    let project = TestProject::new();
    project.write("top.syl", CLEAN);
    success(&project.run(
        ".",
        &[
            "emit",
            "--out-dir",
            "generated sv",
            "--filelist",
            "custom.f",
            "top.syl",
        ],
    ));
    let module = project.0.join("generated sv/Top.sv");
    assert_eq!(
        fs::read_to_string(project.0.join("custom.f")).unwrap(),
        format!("\"{}\"\n", module.display())
    );
    lint_filelist(&project.0, "custom.f");
}

#[cfg(unix)]
#[test]
fn custom_filelist_rejects_symlinks_to_module_outputs() {
    let project = TestProject::new();
    project.write("top.syl", CLEAN);
    std::os::unix::fs::symlink("sv/Top.sv", project.0.join("custom.f")).unwrap();
    let module = project.0.join("sv/Top.sv");
    for existing in [false, true] {
        if existing {
            fs::write(&module, "previous output").unwrap();
        }
        let output = project.run(
            ".",
            &["--out-dir", "sv", "--filelist", "custom.f", "top.syl"],
        );
        failure(&output);
        assert!(String::from_utf8_lossy(&output.stderr).contains("filelist path collides"));
        if existing {
            assert_eq!(fs::read_to_string(&module).unwrap(), "previous output");
        } else {
            assert!(!module.exists());
        }
    }
}

#[test]
fn sim_runs_json_drives_edges_async_reset_and_vcd() {
    let project = TestProject::new();
    project.write(
        "top.syl",
        "cell Top<W: nat = 4,>(clk:in Clock,rst:in Reset<Async,ActiveLow>,i:in UInt<W>)-> q:UInt<W> {
        reg stored:UInt<W> reset(rst,0)
        next stored := i
        q := stored
    }",
    );
    project.write(
        "steps.json",
        r#"[{"drive":{"rst":0,"i":7},"expect":{"q":0}},
        {"drive":{"rst":1},"expect":{"q":0}},{"clocks":["clk"],"expect":{"q":7}},
        {"drive":{"i":"zzx10"},"clocks":["clk"],"expect":{"q":"zzx10"}},
        {"drive":{"rst":0},"expect":{"q":0}}]"#,
    );
    let out = project.run(
        "",
        &[
            "sim",
            "top.syl",
            "--top",
            "Top",
            "--param",
            "W=5",
            "--stimulus",
            "steps.json",
            "--vcd",
            "wave.vcd",
        ],
    );
    success(&out);
    assert!(String::from_utf8(out.stdout).unwrap().contains("q=00000"));
    let wave = fs::read_to_string(project.0.join("wave.vcd")).unwrap();
    assert!(
        wave.contains("$scope module Top $end") && wave.contains("bzzx10 ") && wave.contains("#8")
    );
    success(&project.run(
        "",
        &[
            "sim",
            "top.syl",
            "--top",
            "Top",
            "--cycles",
            "2",
            "--fail-on-x",
        ],
    ));
    let out = project.run(
        "",
        &[
            "sim",
            "top.syl",
            "--top",
            "Top",
            "--stimulus",
            "steps.json",
            "--param",
            "W=5",
            "--fail-on-x",
        ],
    );
    failure(&out);
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("unknown output q")
    );
}

#[test]
fn sim_errors_report_assertion_unique_and_cycle_source_lines() {
    let p = TestProject::new();
    p.write(
        "guard.syl",
        "cell Top(clk:in Clock,i:in Bit)-> y:Bit {\n    assert(i)\n    y:=i\n}",
    );
    p.write("steps.json", r#"[{"drive":{"i":0},"clocks":["clk"]}]"#);
    let args = [
        "sim",
        "guard.syl",
        "--top",
        "Top",
        "--stimulus",
        "steps.json",
        "--diagnostics-format",
        "json",
    ];
    let out = p.run("", &args);
    failure(&out);
    let json = report(&out);
    let e = &json["diagnostics"][0];
    assert_eq!(e["location"]["start_line"], 2);
    assert_eq!(e["location"]["start_column"], 5);
    assert_eq!(e["location"]["file"], "guard.syl");
    assert!(
        e["message"]
            .as_str()
            .unwrap()
            .contains("Top.assert[0]: assert failed")
    );
    p.write(
        "unique.syl",
        "cell Top(i:in Bit)->y:Bit {\n y:= select unique { i=>1,i=>0, default=>0,}\n}",
    );
    p.write("steps.json", r#"[{"drive":{"i":1},"expect":{"y":1}}]"#);
    let out = p.run(
        "",
        &[
            "sim",
            "unique.syl",
            "--top",
            "Top",
            "--stimulus",
            "steps.json",
            "--strict-unique",
        ],
    );
    failure(&out);
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("unique.syl:2:")
    );
    p.write("cycle.syl","cell Child(i:in Bit)->o:Bit{o:=i}\ncell Top()->y:Bit{signal a:Bit\n let c=place Child(i:a)\n a:=c\n y:=a\n}");
    let out = p.run(
        "",
        &[
            "sim",
            "cycle.syl",
            "--top",
            "Top",
            "--diagnostics-format",
            "json",
        ],
    );
    failure(&out);
    let json = report(&out);
    let e = &json["diagnostics"][0];
    assert_eq!(e["location"]["file"], "cycle.syl");
    assert!(e["location"]["start_line"].as_u64().unwrap() > 0);
    assert!(e["message"].as_str().unwrap().contains("cycle"));
}
