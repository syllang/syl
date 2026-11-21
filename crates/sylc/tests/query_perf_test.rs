//! Ignored CLI emit measurement. `cargo test` does not run this.
//!
//! ```text
//! cargo test -p sylc --release --test query_perf_test -- --ignored --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::time::Instant;

use syl_emit::SystemVerilogBackend;
use syl_query::AnalysisQueries;
use syl_session::{AnalysisHost, CancellationToken, DocumentUri, DocumentVersion, ProjectConfig};
use syl_span::SourcePosition;

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
    let mut query = Vec::with_capacity(IDE_SAMPLES);
    let query_snapshot = query_snapshot();
    for _ in 0..IDE_SAMPLES {
        query.push(query_once(&query_snapshot));
    }
    query.sort_unstable();
    let query_p50 = rank(&query, 0.50);
    let query_p95 = rank(&query, 0.95);
    println!(
        "SYL_PERF_EMIT {{\"profile\":\"{profile}\",\"cold_mvp_emit_us\":{{\"n\":{},\"p50\":{p50},\"p95\":{p95}}},\"repeat_query_us\":{{\"n\":{},\"p50\":{query_p50},\"p95\":{query_p95}}}}}",
        samples.len(),
        query.len()
    );
}

const IDE_SAMPLES: usize = 21;

struct QueryFixture {
    snapshot: syl_session::AnalysisSnapshot,
    uri: DocumentUri,
    source: &'static str,
}

fn query_snapshot() -> QueryFixture {
    let source = "cell Top(x: in Bit, y: out Bit) {\n    y := x\n}\n";
    let uri = DocumentUri::new("untitled:syl/perf-query");
    let mut host = AnalysisHost::new();
    host.open_document(uri.clone(), source.to_string(), DocumentVersion::new(1));
    let snapshot = host.snapshot().expect("query snapshot");
    QueryFixture {
        snapshot,
        uri,
        source,
    }
}

fn query_once(fixture: &QueryFixture) -> u128 {
    let token = CancellationToken::new();
    let started = Instant::now();
    let hover = fixture
        .snapshot
        .hover_at_with_token(&fixture.uri, utf16_at(fixture.source, "x\n}"), &token)
        .expect("hover");
    let definition = fixture
        .snapshot
        .definition_at_with_token(&fixture.uri, utf16_at(fixture.source, "x\n}"), &token)
        .expect("definition");
    let completions = fixture
        .snapshot
        .completions_at_with_token(&fixture.uri, utf16_at(fixture.source, "y := "), &token)
        .expect("completion");
    let diagnostics = fixture
        .snapshot
        .document_diagnostics_with_token(&fixture.uri, &token)
        .expect("diagnostics");
    let elapsed = started.elapsed().as_micros();
    assert!(hover.is_some(), "hover must hit the query API");
    assert!(definition.is_some(), "definition must hit the query API");
    assert!(
        !completions.items.is_empty(),
        "completion must hit the query API"
    );
    assert!(diagnostics.is_some(), "diagnostics must hit the query API");
    elapsed
}

fn utf16_at(source: &str, needle: &str) -> SourcePosition {
    let offset = source
        .find(needle)
        .unwrap_or_else(|| panic!("missing {needle}"));
    let prefix = &source[..offset];
    let line = prefix.lines().count().saturating_sub(1);
    let character = prefix
        .lines()
        .last()
        .map(|line| line.encode_utf16().count())
        .unwrap_or(0);
    SourcePosition::new(line, character)
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
