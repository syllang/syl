use std::{io::ErrorKind, path::Path, process::Command};

#[test]
fn emitted_std_components_pass_cycle_simulation() {
    match Command::new("verilator").arg("--version").output() {
        Err(err) if err.kind() == ErrorKind::NotFound => {
            eprintln!("Skipping std simulation: verilator is missing");
            return;
        }
        result => assert!(result.expect("run verilator").status.success()),
    }
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check_std_sim.sh");
    let status = Command::new("bash")
        .arg(script)
        .env("SYLC", env!("CARGO_BIN_EXE_sylc"))
        .status()
        .expect("run std simulation script");
    assert!(status.success(), "std simulation failed: {status}");
}
