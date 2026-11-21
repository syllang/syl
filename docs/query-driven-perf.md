# Query-driven cutover measurements

Measured on 2026-10-07. The harness is ignored so `scripts/quality_gate.sh` does not run it.

- Machine: AMD Ryzen 9 7950X, Linux 7.0.13-arch1-1, 32 hardware threads
- Compiler: rustc 1.96.0 (ac68faa20 2026-05-25)
- Profile: `release`
- Samples: 7 cold or edit iterations, 21 repeated IDE iterations
- Percentile: nearest rank, index `round((n - 1) * fraction)` on the sorted samples
- MVP input: `examples/mvp` with std root `examples/std`
- Edit fixture: `lib.syl` cell `Alpha` plus `app.syl` cell `Beta`

Reproduce from a clean release build:

```text
cargo test -p syl_session --release --lib query_cutover_measurements -- --ignored --nocapture
cargo test -p sylc --release --test query_perf_test -- --ignored --nocapture
```

Each command prints one `SYL_PERF_SESSION` or `SYL_PERF_EMIT` JSON line. `n=7` makes p95 the maximum sample. Repeated IDE times of 0 or 1 µs are memo hits under the timer resolution.

## Regression budget

WP0 did not leave a checked-in baseline. There is no artifact from before the cutover that shows these caps being chosen against the old cache, which remains an explicit evidence limit for these measurements. The numbers below are a regression budget for the shipping tree, written down on 2026-10-07 after that tree existed. A later run can reproduce the shipping tree. It cannot turn these caps into a pre-cutover Go/No-Go.

The caps are:

- cold MVP load, diagnostics, HWIR, and SystemVerilog emit: p50 ≤ 31000 µs, p95 ≤ 42000 µs
- four live MVP snapshots kept at once: `VmRSS` and `VmHWM` ≤ 40000 KB

The shipping tree meets both. That tree is the module split (`e63394a`) plus the const-cycle guard and the same-key cancellation lock.

| measurement | n | p50 µs | p95 µs |
| --- | ---: | ---: | ---: |
| cold pair, diagnostics and HWIR | 7 | 324 | 1162 |
| repeated IDE on one file | 21 | 1 | 1 |
| body edit `y := 1` to `y := 0`, then HWIR | 7 | 156 | 241 |
| signature edit `return 1` to `return 2`, then HWIR | 7 | 170 | 180 |
| cold MVP diagnostics and HWIR | 7 | 8803 | 13225 |
| cold MVP emit | 7 | 8109 | 13695 |

Four live MVP snapshots: `VmRSS` 32724 KB, `VmHWM` 32724 KB.

On that tree a body edit and a signature edit each ran `owner_module_eir` once and `workspace_elaboration` once (`body_edit_modules=1`, `body_edit_designs=1`, and the same pair for the signature edit).

## WP3 split

`f918f2c` keeps Const MIR, Map IR, EIR, driver facts, and DRC in one elaboration query. `e63394a` splits Const MIR, Map IR, and one module's EIR into their own queries. Driver facts, DRC, and HWIR stay one design aggregate. The comparison copied this harness into throwaway worktrees and did not commit it there. Those commits were reworded after the run; the measured trees are the same.

| measurement | monolithic p50 / p95 | module split p50 / p95 |
| --- | --- | --- |
| cold pair µs | 274 / 970 | 351 / 1066 |
| repeated IDE µs | 0 / 1 | 0 / 1 |
| body edit µs | 137 / 167 | 147 / 171 |
| signature edit µs | 133 / 398 | 155 / 161 |
| cold MVP HWIR µs | 8336 / 12569 | 7917 / 12139 |
| cold MVP emit µs | 8522 / 13717 | 8982 / 13483 |
| four snapshots RSS and HWM KB | 30188 | 32740 |

Query deltas on a body edit: monolithic `owner_module_eir` 0 and `workspace_elaboration` 1; split `owner_module_eir` 1 and `workspace_elaboration` 1. The signature edit has the same deltas. The untouched module's stage queries are the part the split can reuse. On this two-cell fixture and on `examples/mvp`, that reuse is not a user-visible speedup. Emit p50 is a few hundred microseconds slower after the split, and four snapshots use about 2.5 MB more. Both still sit inside the caps above.

The split stays in the tree as part of this branch because the work package was requested after the measurement gate, rather than remaining deferred on a future speedup. This file does not claim a latency win for it.

## Workload added 2026-10-11

Same machine and `release` profile. Commands are unchanged. This run also prints stage demand and the `syl_query` repeat.

`stages` times the first demand after `snapshot()` on the two-cell fixture: file AST, HIR, TIR, then HWIR. Counts are how many times `parsed_file`, `owner_hir`, `owner_tir`, and `workspace_elaboration` ran during that demand, from the last of the seven iterations. A 0 µs sample is under the timer resolution.

`repeat_query_us` is 21 calls on one snapshot through `hover_at_with_token`, `definition_at_with_token`, `completions_at_with_token`, and `document_diagnostics_with_token`. The older `repeat_ide_us` field is still the session facade (HIR, TIR, package stage diagnostics, and AST). Both are memo hits at this resolution.

```text
SYL_PERF_SESSION {"profile":"release","cold_pair_us":{"n":7,"p50":308,"p95":1208},"repeat_ide_us":{"n":21,"p50":1,"p95":6},"body_edit_us":{"n":7,"p50":144,"p95":156},"signature_edit_us":{"n":7,"p50":153,"p95":162},"cold_mvp_hwir_us":{"n":7,"p50":8493,"p95":12664},"body_edit_modules":1,"body_edit_designs":1,"signature_edit_modules":1,"signature_edit_designs":1,"four_snapshot_rss_kb":32772,"four_snapshot_hwm_kb":32772,"stages":{"n":7,"parse_us":{"n":7,"p50":0,"p95":0},"hir_us":{"n":7,"p50":37,"p95":37},"tir_us":{"n":7,"p50":9,"p95":9},"elab_us":{"n":7,"p50":119,"p95":122},"parse_count":2,"hir_count":3,"tir_count":3,"elab_count":1}}
SYL_PERF_EMIT {"profile":"release","cold_mvp_emit_us":{"n":7,"p50":8116,"p95":14344},"repeat_query_us":{"n":21,"p50":2,"p95":3}}
```

Cold MVP HWIR, cold MVP emit, and the four snapshots in this run stay inside the caps above. `n=7` still makes p95 the maximum sample.


## Wave 1: shared parsing and 50/100-file workspaces

Measured on 2026-11-21 against WP-1C, based on `69ace8b`. Machine: AMD Ryzen 9
7950X, 32 hardware threads, Linux 7.2.6-arch2-1, rustc 1.96.0
(ac68faa20 2026-05-25), `release` profile. Each scenario has seven samples;
percentiles use `round((n - 1) * fraction)`, so p95 is the maximum of seven.
The measurements below were run sequentially after compilation.

The resolver now stages each discovered source into Salsa before following its
imports. `parsed_file` owns parsing and shares an `Arc` containing the AST,
node index, and parse diagnostics with the snapshot. An unchanged input uses
its existing memo; cloning snapshots shares their parsed files. Publication
invalidates older generations before the first input mutation, so cancellation
between files or inside a parse query leaves older snapshots isolated and a
subsequent publication can retry. Public method signatures are unchanged.

The shared `scale_fixture_tests.rs` generator creates temporary four-layer DAGs
with 5/10 definition files, 20/40 leaf files, 20/40 subsystem files, and 5/10 top
files. Each definition file has two constants, an enum, a bundle, a width
function, and a map. Each leaf has a combinational cell using the imported
constant, bundle, and map, plus a clock/reset/register cell. Subsystems combine
`place` and `inplace` calls to two leaf files; each top instantiates four
subsystems. Names are unique across the workspace. The resulting HWIR contains
65/130 modules, which both harnesses assert; the emit harness also checks the
number of emitted modules and stable output length across samples.

Only top files are roots, so loading must discover the rest through imports.
Fixture creation and disk edits are outside the measured interval. Cold time
includes host construction, snapshot loading, semantic diagnostics, and HWIR;
the CLI test also calls `SystemVerilogBackend::emit`. Each edit starts from a
fresh, warmed host and includes a new snapshot plus the requested queries.
The IDE scenario requests HIR, TIR, and grouped diagnostics for `leaf_0.syl`.
Body edits change `eq 0` to `eq 1` without moving spans; signature edits change
`W_0` from 8 to 9, affecting four combinational leaves. Session counts come
from its test-only Salsa execution log; the emit harness uses the public API.

Reproduce:

```text
cargo test -p syl_session --release --lib scale_workspace_measurements -- --ignored --nocapture
cargo test -p sylc --release --test query_perf_test scale_emit_measurements -- --ignored --nocapture
cargo test -p syl_session --release --lib query_cutover_measurements -- --ignored --nocapture
cargo test -p sylc --release --test query_perf_test cold_mvp_emit_measurements -- --ignored --nocapture
```

| Scenario | 50 files p50 / p95 µs | 100 files p50 / p95 µs |
| --- | ---: | ---: |
| Cold load through HWIR | 54374 / 75355 | 131332 / 140093 |
| Leaf body edit, IDE queries | 1629 / 1691 | 3039 / 3210 |
| Leaf body edit, HWIR | 14585 / 18783 | 38701 / 44046 |
| Constant signature edit, HWIR | 23154 / 26931 | 50812 / 53156 |
| Cold load through SV emit | 61788 / 89613 | 142965 / 167579 |

Cold parse counts are exactly 50 and 100. The edit query deltas below are the
same at both sizes and are checked for equality across all seven samples.

| Edit | Parse | Owner HIR | Owner TIR | Owner module EIR | Workspace elaboration |
| --- | ---: | ---: | ---: | ---: | ---: |
| Leaf body, IDE | 1 | 1 | 1 | 0 | 0 |
| Leaf body, HWIR | 1 | 1 | 1 | 1 | 1 |
| Constant signature, HWIR | 1 | 20 | 20 | 4 | 1 |

Process RSS/HWM after the scale scenarios is 59488/59488 KB at 50 files and
101592/101592 KB at 100 files. These are `/proc/self/status` readings after the
scenario snapshots have been dropped; allocator retention and earlier samples
remain reflected in them. Both sizes run in one process, so HWM is cumulative.
They are not the four-live-MVP-snapshot metric governed by the 40000 KB cap.

The existing MVP checks also remain within their budgets: cold HWIR p50/p95
7966/12984 µs, cold emit 8619/14796 µs, and four live snapshots RSS/HWM
33308/33308 KB. The two-file stage sample records `snapshot_parse_count = 2`
and `post_snapshot_parse_count = 0`; subsequent semantic/HWIR demand does not
parse those files again.

Regression requirements:

- The non-ignored 10-file test covers both `AnalysisHost` and `ProjectResolver`:
  cold load plus semantics/HWIR parses exactly N files, an unchanged snapshot
  adds zero parses, and a single leaf edit adds exactly one. AST and node-index
  pointer checks verify sharing; old AST/text/span data remains immutable.
- The non-ignored cancellation test interrupts after one changed file is
  staged, both between files and at a second changed file's parse query. The
  older snapshot still produces its original design; a fresh token retries
  successfully. Cancelled execution attempts are counted separately in that
  test and are not successful parse measurements.
- Every scale sample asserts `cold_parse_count == file_count`, edit
  `parse_delta == 1`, no elaboration for IDE demand, and exactly one workspace
  elaboration for each HWIR edit.
- Keep the existing MVP latency and 40000 KB four-snapshot caps above. Scale
  timings and memory are a first measured baseline, not a before/after speedup
  claim or an extension of the MVP memory cap to larger workspaces.

Raw output:

```text
SYL_PERF_SCALE {"profile":"release","file_count":50,"cold_load_hwir_us":{"n":7,"p50":54374,"p95":75355},"cold_parse_count":50,"leaf_body_edit_ide_us":{"n":7,"p50":1629,"p95":1691},"leaf_body_edit_ide_counts":{"parse_delta":1,"owner_hir_delta":1,"owner_tir_delta":1,"owner_module_eir_delta":0,"workspace_elab_delta":0},"leaf_body_edit_hwir_us":{"n":7,"p50":14585,"p95":18783},"leaf_body_edit_hwir_counts":{"parse_delta":1,"owner_hir_delta":1,"owner_tir_delta":1,"owner_module_eir_delta":1,"workspace_elab_delta":1},"sig_edit_hwir_us":{"n":7,"p50":23154,"p95":26931},"sig_edit_hwir_counts":{"parse_delta":1,"owner_hir_delta":20,"owner_tir_delta":20,"owner_module_eir_delta":4,"workspace_elab_delta":1},"rss_kb":59488,"hwm_kb":59488}
SYL_PERF_SCALE {"profile":"release","file_count":100,"cold_load_hwir_us":{"n":7,"p50":131332,"p95":140093},"cold_parse_count":100,"leaf_body_edit_ide_us":{"n":7,"p50":3039,"p95":3210},"leaf_body_edit_ide_counts":{"parse_delta":1,"owner_hir_delta":1,"owner_tir_delta":1,"owner_module_eir_delta":0,"workspace_elab_delta":0},"leaf_body_edit_hwir_us":{"n":7,"p50":38701,"p95":44046},"leaf_body_edit_hwir_counts":{"parse_delta":1,"owner_hir_delta":1,"owner_tir_delta":1,"owner_module_eir_delta":1,"workspace_elab_delta":1},"sig_edit_hwir_us":{"n":7,"p50":50812,"p95":53156},"sig_edit_hwir_counts":{"parse_delta":1,"owner_hir_delta":20,"owner_tir_delta":20,"owner_module_eir_delta":4,"workspace_elab_delta":1},"rss_kb":101592,"hwm_kb":101592}
SYL_PERF_SCALE_EMIT {"profile":"release","file_count":50,"cold_load_emit_us":{"n":7,"p50":61788,"p95":89613},"emitted_bytes":15023}
SYL_PERF_SCALE_EMIT {"profile":"release","file_count":100,"cold_load_emit_us":{"n":7,"p50":142965,"p95":167579},"emitted_bytes":30048}
SYL_PERF_SESSION {"profile":"release","cold_pair_us":{"n":7,"p50":321,"p95":1270},"repeat_ide_us":{"n":21,"p50":0,"p95":1},"body_edit_us":{"n":7,"p50":140,"p95":174},"signature_edit_us":{"n":7,"p50":156,"p95":159},"cold_mvp_hwir_us":{"n":7,"p50":7966,"p95":12984},"body_edit_modules":1,"body_edit_designs":1,"signature_edit_modules":1,"signature_edit_designs":1,"four_snapshot_rss_kb":33308,"four_snapshot_hwm_kb":33308,"stages":{"n":7,"parse_us":{"n":7,"p50":0,"p95":0},"hir_us":{"n":7,"p50":65,"p95":68},"tir_us":{"n":7,"p50":24,"p95":28},"elab_us":{"n":7,"p50":101,"p95":106},"snapshot_parse_count":2,"post_snapshot_parse_count":0,"hir_count":4,"tir_count":4,"elab_count":1}}
SYL_PERF_EMIT {"profile":"release","cold_mvp_emit_us":{"n":7,"p50":8619,"p95":14796},"repeat_query_us":{"n":21,"p50":2,"p95":3}}
```
