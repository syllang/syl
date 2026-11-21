use std::{io::ErrorKind, path::Path, process::Command};

#[test]
fn emitted_std_components_pass_cycle_simulation_including_cdc_ram_and_fifo() {
    match Command::new("verilator").arg("--version").output() {
        Err(err) if err.kind() == ErrorKind::NotFound => {
            eprintln!("Skipping std simulation: verilator is missing");
            return;
        }
        result => assert!(result.expect("run verilator").status.success()),
    }
    // The shared script builds and runs tests/sim/<package>_test.cpp against emitted SV.
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/check_std_sim.sh");
    let output = Command::new("bash")
        .arg(script)
        .env("SYLC", env!("CARGO_BIN_EXE_sylc"))
        .output()
        .expect("run std simulation script");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "std simulation failed: {}\nstdout:\n{stdout}\nstderr:\n{}",
        output.status,
        String::from_utf8_lossy(&output.stderr)
    );
    // Check the public script actually executed every component, including
    // both reset polarities, all RAM read modes, and the stream FIFO wrapper.
    for top in [
        "EdgeDetector",
        "PulseStretcher",
        "WordSkidBuffer",
        "FixedPriorityArbiter4",
        "RoundRobinArbiter4",
        "Sync2FF",
        "Sync2FFAsyncLow",
        "PulseSynchronizer",
        "SinglePortRam",
        "SimpleDualPortRam",
        "DistributedRam",
        "SyncFifo",
        "StreamSyncFifo",
    ] {
        assert!(
            stdout.lines().any(|line| line == format!("PASS {top}")),
            "missing cycle simulation for {top}:\n{stdout}"
        );
    }
}
