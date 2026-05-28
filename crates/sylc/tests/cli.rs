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
