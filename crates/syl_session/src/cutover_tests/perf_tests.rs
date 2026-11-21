//! Parse-publication regressions and ignored measurements for the cutover budget.
//!
//! ```text
//! cargo test -p syl_session --release --lib query_cutover_measurements -- --ignored --nocapture
//! ```

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::{AnalysisHost, AnalysisSnapshot, CancellationToken, DocumentUri, ProjectConfig};

#[path = "scale_fixture_tests.rs"]
mod scale_fixture_tests;

use super::TempTree;
use scale_fixture_tests::ScaleFixture;

const SAMPLES: usize = 7;
const IDE_SAMPLES: usize = 21;

#[test]
#[ignore = "prints SYL_PERF_SESSION json; not part of the quality gate"]
fn query_cutover_measurements() {
    let cold = samples(SAMPLES, cold_pair);
    let ide = repeat_ide();
    let stages = stage_samples();
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
        "SYL_PERF_SESSION {{\"profile\":\"{profile}\",\"cold_pair_us\":{},\"repeat_ide_us\":{},\"body_edit_us\":{},\"signature_edit_us\":{},\"cold_mvp_hwir_us\":{},\"body_edit_modules\":{},\"body_edit_designs\":{},\"signature_edit_modules\":{},\"signature_edit_designs\":{},\"four_snapshot_rss_kb\":{},\"four_snapshot_hwm_kb\":{},\"stages\":{}}}",
        stats(&cold),
        stats(&ide.times),
        stats(&body.times),
        stats(&signature.times),
        stats(&mvp),
        body.module_delta,
        body.design_delta,
        signature.module_delta,
        signature.design_delta,
        memory.0,
        memory.1,
        stages,
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

struct IdeSample {
    times: Vec<u128>,
}

fn repeat_ide() -> IdeSample {
    let fixture = Pair::new();
    let snapshot = fixture.snapshot();
    let uri = app_uri(&snapshot);
    let mut times = Vec::with_capacity(IDE_SAMPLES);
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
        times.push(elapsed_us(started.elapsed()));
    }
    IdeSample { times }
}

fn stage_samples() -> String {
    let mut parse_us = Vec::with_capacity(SAMPLES);
    let mut hir_us = Vec::with_capacity(SAMPLES);
    let mut tir_us = Vec::with_capacity(SAMPLES);
    let mut elab_us = Vec::with_capacity(SAMPLES);
    let mut counts = StageCounts::default();
    for _ in 0..SAMPLES {
        let fixture = Pair::new();
        let snapshot = fixture.snapshot();
        let uri = app_uri(&snapshot);
        let token = CancellationToken::new();
        let parsed_before = snapshot.execution_count("parsed_file(");
        let hir_before = snapshot.execution_count("owner_hir(");
        let tir_before = snapshot.execution_count("owner_tir(");
        let elab_before = snapshot.execution_count("workspace_elaboration(");
        let started = Instant::now();
        let _ = snapshot.file_by_uri(&uri).expect("file").ast();
        parse_us.push(elapsed_us(started.elapsed()));
        let started = Instant::now();
        snapshot
            .hir_analysis_for_uri_with_token(&uri, &token)
            .expect("hir")
            .expect("hir stage");
        hir_us.push(elapsed_us(started.elapsed()));
        let started = Instant::now();
        snapshot
            .tir_analysis_for_uri_with_token(&uri, &token)
            .expect("tir")
            .expect("tir stage");
        tir_us.push(elapsed_us(started.elapsed()));
        let started = Instant::now();
        let _ = snapshot.hwir();
        elab_us.push(elapsed_us(started.elapsed()));
        counts = StageCounts {
            snapshot_parse: parsed_before,
            post_snapshot_parse: snapshot.execution_count("parsed_file(") - parsed_before,
            hir: snapshot.execution_count("owner_hir(") - hir_before,
            tir: snapshot.execution_count("owner_tir(") - tir_before,
            elab: snapshot.execution_count("workspace_elaboration(") - elab_before,
        };
        assert_eq!(counts.snapshot_parse, 2);
        assert_eq!(counts.post_snapshot_parse, 0);
    }
    format!(
        "{{\"n\":{SAMPLES},\"parse_us\":{},\"hir_us\":{},\"tir_us\":{},\"elab_us\":{},\"snapshot_parse_count\":{},\"post_snapshot_parse_count\":{},\"hir_count\":{},\"tir_count\":{},\"elab_count\":{}}}",
        stats(&parse_us),
        stats(&hir_us),
        stats(&tir_us),
        stats(&elab_us),
        counts.snapshot_parse,
        counts.post_snapshot_parse,
        counts.hir,
        counts.tir,
        counts.elab,
    )
}

#[derive(Default)]
struct StageCounts {
    snapshot_parse: usize,
    post_snapshot_parse: usize,
    hir: usize,
    tir: usize,
    elab: usize,
}

fn app_uri(snapshot: &AnalysisSnapshot) -> DocumentUri {
    snapshot
        .files()
        .iter()
        .find(|file| file.module_path() == ["app".to_string()])
        .expect("app module")
        .uri()
        .clone()
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

#[test]
#[ignore = "prints SYL_PERF_SCALE json; not part of the quality gate"]
fn scale_workspace_measurements() {
    for file_count in [50, 100] {
        let mut cold = Vec::with_capacity(SAMPLES);
        for _ in 0..SAMPLES {
            let fixture = ScaleFixture::new(file_count);
            let started = Instant::now();
            let snapshot = scale_host(&fixture)
                .snapshot()
                .expect("cold scale snapshot");
            assert_scale_hwir(&snapshot, file_count);
            cold.push(started.elapsed().as_micros());
            assert_eq!(snapshot.execution_count("parsed_file("), file_count);
        }
        let ide = scale_edits(file_count, ScaleEdit::BodyIde);
        let body = scale_edits(file_count, ScaleEdit::BodyHwir);
        let signature = scale_edits(file_count, ScaleEdit::SignatureHwir);
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        println!(
            "SYL_PERF_SCALE {{\"profile\":\"{profile}\",\"file_count\":{file_count},\"cold_load_hwir_us\":{},\"cold_parse_count\":{file_count},\"leaf_body_edit_ide_us\":{},\"leaf_body_edit_ide_counts\":{},\"leaf_body_edit_hwir_us\":{},\"leaf_body_edit_hwir_counts\":{},\"sig_edit_hwir_us\":{},\"sig_edit_hwir_counts\":{},\"rss_kb\":{},\"hwm_kb\":{}}}",
            stats(&cold),
            stats(&ide.times),
            ide.counts,
            stats(&body.times),
            body.counts,
            stats(&signature.times),
            signature.counts,
            vm_kb("VmRSS:"),
            vm_kb("VmHWM:"),
        );
    }
}

fn scale_host(fixture: &ScaleFixture) -> AnalysisHost {
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(fixture.root.clone()));
    host.set_roots(fixture.roots.clone());
    host
}

fn assert_scale_hwir(snapshot: &AnalysisSnapshot, file_count: usize) {
    assert_eq!(snapshot.files().len(), file_count);
    assert!(
        snapshot.diagnostics().is_empty(),
        "{:?}",
        snapshot.diagnostics()
    );
    let diagnostics = snapshot.semantic_diagnostics();
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    let hwir = snapshot.hwir().expect("scale HWIR");
    // Four pairs of leaf cells, four mid cells, and one top per ten files.
    assert_eq!(hwir.modules().len(), file_count / 10 * 13);
}

#[derive(Clone, Copy)]
enum ScaleEdit {
    BodyIde,
    BodyHwir,
    SignatureHwir,
}

struct ScaleEditSamples {
    times: Vec<u128>,
    counts: String,
}

fn scale_edits(file_count: usize, edit: ScaleEdit) -> ScaleEditSamples {
    let mut times = Vec::with_capacity(SAMPLES);
    let mut counts = String::new();
    for _ in 0..SAMPLES {
        let fixture = ScaleFixture::new(file_count);
        let mut host = scale_host(&fixture);
        let older = host.snapshot().expect("baseline scale snapshot");
        let uri = DocumentUri::from_file_path(&fixture.root.join("pkg_leaf/leaf_0.syl"));
        if matches!(edit, ScaleEdit::BodyIde) {
            scale_ide(&older, &uri);
        } else {
            assert_scale_hwir(&older, file_count);
        }
        let before = scale_counts(&older);
        let (path, from, to) = if matches!(edit, ScaleEdit::SignatureHwir) {
            ("pkg_defs/defs_0.syl", "W_0: nat = 8", "W_0: nat = 9")
        } else {
            ("pkg_leaf/leaf_0.syl", "eq 0", "eq 1")
        };
        let path = fixture.root.join(path);
        let source = fs::read_to_string(&path).unwrap();
        assert!(source.contains(from));
        fs::write(path, source.replace(from, to)).unwrap();
        let started = Instant::now();
        let newer = host.snapshot().expect("edited scale snapshot");
        if matches!(edit, ScaleEdit::BodyIde) {
            scale_ide(&newer, &uri);
        } else {
            assert_scale_hwir(&newer, file_count);
        }
        times.push(started.elapsed().as_micros());
        let after = scale_counts(&newer);
        let delta: Vec<_> = after.iter().zip(before).map(|(a, b)| a - b).collect();
        assert_eq!(delta[0], 1, "an edit parses exactly one file");
        let ide = matches!(edit, ScaleEdit::BodyIde);
        assert_eq!(delta[4], usize::from(!ide));
        if ide {
            assert_eq!(delta[3], 0);
            assert_eq!(newer.execution_count("unit_elaboration("), 0);
        } else {
            assert!(delta[3] > 0, "the hardware edit must reach module EIR");
        }
        let measured = format!(
            "{{\"parse_delta\":{},\"owner_hir_delta\":{},\"owner_tir_delta\":{},\"owner_module_eir_delta\":{},\"workspace_elab_delta\":{}}}",
            delta[0], delta[1], delta[2], delta[3], delta[4],
        );
        if !counts.is_empty() {
            assert_eq!(counts, measured, "query deltas must be deterministic");
        }
        counts = measured;
    }
    ScaleEditSamples { times, counts }
}

fn scale_counts(snapshot: &AnalysisSnapshot) -> [usize; 5] {
    [
        "parsed_file(",
        "owner_hir(",
        "owner_tir(",
        "owner_module_eir(",
        "workspace_elaboration(",
    ]
    .map(|name| snapshot.execution_count(name))
}

fn scale_ide(snapshot: &AnalysisSnapshot, uri: &DocumentUri) {
    assert!(snapshot.diagnostics().is_empty());
    let token = CancellationToken::new();
    snapshot
        .hir_analysis_for_uri_with_token(uri, &token)
        .unwrap()
        .expect("scale HIR");
    snapshot
        .tir_analysis_for_uri_with_token(uri, &token)
        .unwrap()
        .expect("scale TIR");
    let diagnostics = snapshot
        .package_stage_diagnostics_with_token(uri, &token)
        .unwrap()
        .unwrap();
    assert!(diagnostics.parse().is_empty());
    assert!(diagnostics.hir().is_empty(), "{:?}", diagnostics.hir());
    assert!(diagnostics.tir().is_empty(), "{:?}", diagnostics.tir());
    assert!(diagnostics.elaboration().is_empty());
}

#[test]
fn cancelled_scale_publication_preserves_old_snapshot_and_retries() {
    use std::cell::Cell;
    use std::collections::BTreeMap;
    use std::rc::Rc;

    use crate::query::QueryStore;
    use crate::vfs::{FsVfs, Vfs};
    use crate::{ProjectError, ProjectResolver};
    use syl_sema::OpaqueSummaryTable;

    #[derive(Debug)]
    struct CancelVfs {
        token: CancellationToken,
        reads: Cell<usize>,
        cancel_at: Rc<Cell<usize>>,
    }
    impl Vfs for CancelVfs {
        fn read_to_string(&self, path: &Path) -> Result<String, ProjectError> {
            self.reads.set(self.reads.get() + 1);
            if self.reads.get() == self.cancel_at.get() {
                self.token.cancel();
            }
            FsVfs.read_to_string(path)
        }
        fn exists(&self, path: &Path) -> bool {
            FsVfs.exists(path)
        }
        fn canonicalize(&self, path: &Path) -> Result<PathBuf, ProjectError> {
            FsVfs.canonicalize(path)
        }
    }

    for cancel_in_query in [false, true] {
        let fixture = ScaleFixture::new(10);
        let token = CancellationToken::new();
        let cancel_at = Rc::new(Cell::new(usize::MAX));
        let resolver = ProjectResolver::with_vfs(
            ProjectConfig::new().with_workspace_root(fixture.root.clone()),
            CancelVfs {
                token: token.clone(),
                reads: Cell::new(0),
                cancel_at: Rc::clone(&cancel_at),
            },
        );
        let mut query = QueryStore::new();
        let opaque = OpaqueSummaryTable::new();
        let resolve = |query: &mut QueryStore, token: &CancellationToken| {
            resolver.snapshot_into_query(
                (fixture.roots.clone(), &BTreeMap::new()),
                query,
                &opaque,
                token,
            )
        };
        let resolved = resolve(&mut query, &token).unwrap();
        let older = AnalysisSnapshot::from_query(query.generation(), query.inflight(), resolved);
        let path = &fixture.roots[0];
        let uri = DocumentUri::from_file_path(path);
        let source = fs::read_to_string(path).unwrap();
        let ast = older.file_by_uri(&uri).unwrap().ast().clone();
        fs::write(path, source.replace("y := s3", "y := s3 xor 0")).unwrap();
        if cancel_in_query {
            let mid = fixture.root.join("pkg_mid/mid_0.syl");
            let source = fs::read_to_string(&mid).unwrap();
            fs::write(mid, format!("{source}\n")).unwrap();
        }
        cancel_at.set(12); // Ten baseline reads, then cancel after the edited top was staged.
        assert!(matches!(
            resolve(&mut query, &token),
            Err(ProjectError::Cancelled)
        ));
        // A cancelled query logs its execution attempt before observing the token.
        assert_eq!(
            older.execution_count("parsed_file("),
            11 + usize::from(cancel_in_query)
        );
        assert_eq!(
            older.input_text(uri.as_str()).as_deref(),
            Some(source.as_str())
        );
        assert_eq!(older.file_by_uri(&uri).unwrap().ast(), &ast);
        assert_scale_hwir(&older, 10);
        let resolved = resolve(&mut query, &CancellationToken::new()).expect("retry publication");
        let newer = AnalysisSnapshot::from_query(query.generation(), query.inflight(), resolved);
        assert_scale_hwir(&newer, 10);
        assert_eq!(
            newer.execution_count("parsed_file("),
            11 + 2 * usize::from(cancel_in_query)
        );
        assert!(
            newer
                .input_text(uri.as_str())
                .unwrap()
                .contains("y := s3 xor 0")
        );
    }
}

#[test]
fn parse_deduplication_parses_each_file_once_and_reuses_unchanged_files() {
    let fixture = ScaleFixture::new(10);
    let mut host =
        AnalysisHost::with_config(ProjectConfig::new().with_workspace_root(fixture.root.clone()));
    host.set_roots(fixture.roots.clone());
    let project = crate::ProjectResolver::with_config(
        ProjectConfig::new().with_workspace_root(fixture.root.clone()),
    )
    .load_paths(fixture.roots.clone())
    .expect("resolver DAG project");
    assert_scale_hwir(project.snapshot(), 10);
    assert_eq!(project.snapshot().execution_count("parsed_file("), 10);
    let older = host.snapshot().expect("cold DAG snapshot");
    assert_eq!(older.files().len(), 10);
    assert!(older.diagnostics().is_empty(), "{:?}", older.diagnostics());
    assert!(
        older.semantic_diagnostics().is_empty(),
        "{:?}",
        older.semantic_diagnostics()
    );
    assert!(older.hwir().is_some());
    assert_eq!(older.execution_count("parsed_file("), 10);
    let cloned = older.clone();
    for (original, copy) in older.files().iter().zip(cloned.files()) {
        assert!(std::ptr::eq(original.ast(), copy.ast()));
        assert!(std::ptr::eq(
            original.ast_node_index(),
            copy.ast_node_index()
        ));
    }
    let unchanged = host.snapshot().expect("unchanged DAG snapshot");
    assert!(unchanged.hwir().is_some());
    assert_eq!(unchanged.execution_count("parsed_file("), 10);
    for (original, reused) in older.files().iter().zip(unchanged.files()) {
        assert!(std::ptr::eq(original.ast(), reused.ast()));
    }
    let path = fixture.root.join("pkg_leaf/leaf_0.syl");
    let uri = DocumentUri::from_file_path(&path);
    let original = older.file_by_uri(&uri).unwrap();
    let ast = original.ast().clone();
    let source = fs::read_to_string(&path).unwrap();
    fs::write(&path, source.replace("eq 0", "eq 1")).unwrap();
    let newer = host.snapshot().expect("edited DAG snapshot");
    assert!(newer.semantic_diagnostics().is_empty());
    assert!(newer.hwir().is_some());
    assert_eq!(newer.execution_count("parsed_file("), 11);
    assert_eq!(original.ast(), &ast);
    assert_eq!(
        older
            .source_map()
            .file(original.source_id())
            .unwrap()
            .text(),
        source
    );
    assert_eq!(
        older.input_text(uri.as_str()).as_deref(),
        Some(source.as_str())
    );
    assert_ne!(newer.file_by_uri(&uri).unwrap().ast(), &ast);
    for original in older.files().iter().filter(|file| file.uri() != &uri) {
        assert!(std::ptr::eq(
            original.ast(),
            newer.file_by_uri(original.uri()).unwrap().ast()
        ));
    }
}
