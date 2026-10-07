//! Ignored CLI emit measurement. `cargo test` does not run this.
//!
//! ```text
//! cargo test -p sylc --release --test query_perf_test -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use syl_emit::SystemVerilogBackend;
use syl_session::{AnalysisHost, ProjectConfig};

const SAMPLES: usize = 7;

#[test]
#[ignore = "prints SYL_PERF_EMIT json; not part of the quality gate"]
fn cold_mvp_emit_measurements() {
    let mut samples = Vec::with_capacity(SAMPLES);
    for _ in 0..SAMPLES {
        samples.push(cold_emit_us());
    }
    samples.sort_unstable();
    let p50 = rank(&samples, 0.50);
    let p95 = rank(&samples, 0.95);
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "SYL_PERF_EMIT {{\"profile\":\"{profile}\",\"cold_mvp_emit_us\":{{\"n\":{},\"p50\":{p50},\"p95\":{p95}}}}}",
        samples.len()
    );
}

fn cold_emit_us() -> u128 {
    let workspace = workspace_root();
    let started = Instant::now();
    let mut host = AnalysisHost::with_config(
        ProjectConfig::new()
            .with_workspace_root(workspace.clone())
            .with_std_root(workspace.join("examples/std")),
    );
    let snapshot = host
        .load(&[workspace.join("examples/mvp")])
        .expect("mvp snapshot");
    assert!(snapshot.diagnostics().is_empty());
    assert!(snapshot.semantic_diagnostics().is_empty());
    let hwir = snapshot.hwir().expect("mvp hwir");
    let verilog = SystemVerilogBackend::new().emit(hwir).expect("mvp emit");
    assert!(verilog.contains("module"));
    started.elapsed().as_micros()
}

fn rank(ordered: &[u128], fraction: f64) -> u128 {
    let index = ((ordered.len() - 1) as f64 * fraction).round() as usize;
    ordered[index]
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}
