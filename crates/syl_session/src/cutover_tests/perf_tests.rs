//! Ignored measurements for the cutover budget. `cargo test` does not run this.
//!
//! ```text
//! cargo test -p syl_session --release --lib query_cutover_measurements -- --ignored --exact --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, ProjectConfig};

use super::TempTree;

const SAMPLES: usize = 7;
const IDE_SAMPLES: usize = 21;

#[test]
#[ignore = "prints SYL_PERF_SESSION json; not part of the quality gate"]
fn query_cutover_measurements() {
    let cold = samples(SAMPLES, cold_pair);
    let ide = repeat_ide();
    let body = edit_samples(false);
    let signature = edit_samples(true);
    let mvp = samples(SAMPLES, cold_mvp);
    let memory = four_snapshots();
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    println!(
        "SYL_PERF_SESSION {{\"profile\":\"{profile}\",\"cold_pair_us\":{},\"repeat_ide_us\":{},\"body_edit_us\":{},\"signature_edit_us\":{},\"cold_mvp_hwir_us\":{},\"body_edit_modules\":{},\"body_edit_designs\":{},\"signature_edit_modules\":{},\"signature_edit_designs\":{},\"four_snapshot_rss_kb\":{},\"four_snapshot_hwm_kb\":{}}}",
        stats(&cold),
        stats(&ide),
        stats(&body.times),
        stats(&signature.times),
        stats(&mvp),
        body.module_delta,
        body.design_delta,
        signature.module_delta,
        signature.design_delta,
        memory.0,
        memory.1,
    );
}

fn cold_pair() -> Duration {
    let fixture = Pair::new();
    let started = Instant::now();
    let snapshot = fixture.snapshot();
    let _ = snapshot.semantic_diagnostics();
    let _ = snapshot.hwir();
    started.elapsed()
}

fn cold_mvp() -> Duration {
    let workspace = workspace_root();
    let started = Instant::now();
    let mut host = AnalysisHost::with_config(mvp_config(&workspace));
    let snapshot = host
        .load(&[workspace.join("examples/mvp")])
        .expect("mvp snapshot");
    let _ = snapshot.semantic_diagnostics();
    let _ = snapshot.hwir();
    started.elapsed()
}

fn repeat_ide() -> Vec<u128> {
    let fixture = Pair::new();
    let snapshot = fixture.snapshot();
    let uri = snapshot
        .files()
        .iter()
        .find(|file| file.module_path() == ["app".to_string()])
        .expect("app module")
        .uri()
        .clone();
    let mut samples = Vec::with_capacity(IDE_SAMPLES);
    for _ in 0..IDE_SAMPLES {
        let started = Instant::now();
        let token = CancellationToken::new();
        snapshot
            .hir_analysis_for_uri_with_token(&uri, &token)
            .expect("definition")
            .expect("hir");
        snapshot
            .tir_analysis_for_uri_with_token(&uri, &token)
            .expect("hover")
            .expect("tir");
        snapshot
            .package_stage_diagnostics_with_token(&uri, &token)
            .expect("diagnostics")
            .expect("stages");
        let _ = snapshot.file_by_uri(&uri).expect("symbols").ast();
        samples.push(elapsed_us(started.elapsed()));
    }
    samples
}

struct EditSample {
    times: Vec<u128>,
    module_delta: usize,
    design_delta: usize,
}

fn edit_samples(signature: bool) -> EditSample {
    let mut times = Vec::with_capacity(SAMPLES);
    let mut module_delta = 0;
    let mut design_delta = 0;
    for _ in 0..SAMPLES {
        let fixture = Pair::new();
        let mut host = fixture.host();
        let older = host.snapshot().expect("baseline");
        let _ = older.hwir();
        let modules = older.execution_count("owner_module_eir(");
        let designs = older.execution_count("workspace_elaboration(");
        if signature {
            fixture.write_width_return("return 2");
        } else {
            fixture.write_beta_body("y := 0");
        }
        let started = Instant::now();
        let newer = host.snapshot().expect("edited snapshot");
        let _ = newer.hwir();
        times.push(elapsed_us(started.elapsed()));
        module_delta = newer.execution_count("owner_module_eir(") - modules;
        design_delta = newer.execution_count("workspace_elaboration(") - designs;
    }
    EditSample {
        times,
        module_delta,
        design_delta,
    }
}

fn four_snapshots() -> (u64, u64) {
    let workspace = workspace_root();
    let mut hosts = Vec::new();
    let mut snapshots = Vec::new();
    for _ in 0..4 {
        let mut host = AnalysisHost::with_config(mvp_config(&workspace));
        let snapshot = host
            .load(&[workspace.join("examples/mvp")])
            .expect("mvp snapshot");
        let _ = snapshot.hwir();
        hosts.push(host);
        snapshots.push(snapshot);
    }
    let rss = vm_kb("VmRSS:");
    let hwm = vm_kb("VmHWM:");
    assert_eq!(snapshots.len(), 4);
    drop(hosts);
    (rss, hwm)
}

fn samples(count: usize, measure: fn() -> Duration) -> Vec<u128> {
    (0..count).map(|_| elapsed_us(measure())).collect()
}

fn elapsed_us(duration: Duration) -> u128 {
    duration.as_micros()
}

fn stats(samples: &[u128]) -> String {
    let mut ordered = samples.to_vec();
    ordered.sort_unstable();
    let p50 = rank(&ordered, 0.50);
    let p95 = rank(&ordered, 0.95);
    format!("{{\"n\":{},\"p50\":{p50},\"p95\":{p95}}}", ordered.len())
}

fn rank(ordered: &[u128], fraction: f64) -> u128 {
    let index = ((ordered.len() - 1) as f64 * fraction).round() as usize;
    ordered[index]
}

fn vm_kb(label: &str) -> u64 {
    let status = fs::read_to_string("/proc/self/status").expect("proc status");
    status
        .lines()
        .find_map(|line| line.strip_prefix(label))
        .and_then(|rest| rest.split_whitespace().next())
        .and_then(|value| value.parse().ok())
        .expect("memory counter")
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("workspace root")
}

fn mvp_config(workspace: &Path) -> ProjectConfig {
    ProjectConfig::new()
        .with_workspace_root(workspace.to_path_buf())
        .with_std_root(workspace.join("examples/std"))
}

struct Pair {
    root: TempTree,
}

impl Pair {
    fn new() -> Self {
        let root = TempTree::new();
        fs::write(
            root.path.join("lib.syl"),
            "fn width() -> nat { return 1 }\ncell Alpha(y: out UInt<width>) { y := 1 }\n",
        )
        .unwrap();
        fs::write(
            root.path.join("app.syl"),
            "use lib.Alpha\ncell Beta(y: out Bit) { y := 1 }\n",
        )
        .unwrap();
        Self { root }
    }

    fn host(&self) -> AnalysisHost {
        let mut host = AnalysisHost::with_config(
            ProjectConfig::new().with_workspace_root(self.root.path.clone()),
        );
        host.set_roots(vec![self.root.path.join("app.syl")]);
        host
    }

    fn snapshot(&self) -> AnalysisSnapshot {
        self.host().snapshot().expect("pair snapshot")
    }

    fn write_beta_body(&self, assign: &str) {
        fs::write(
            self.root.path.join("app.syl"),
            format!("use lib.Alpha\ncell Beta(y: out Bit) {{ {assign} }}\n"),
        )
        .unwrap();
    }

    fn write_width_return(&self, returned: &str) {
        fs::write(
            self.root.path.join("lib.syl"),
            format!(
                "fn width() -> nat {{ {returned} }}\ncell Alpha(y: out UInt<width>) {{ y := 1 }}\n"
            ),
        )
        .unwrap();
    }
}
