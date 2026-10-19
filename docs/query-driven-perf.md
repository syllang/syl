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

Each command prints one `SYL_PERF_SESSION` or `SYL_PERF_EMIT` JSON line. `n=7` makes p95 one sample away from the maximum. Repeated IDE times of 0 or 1 µs are memo hits under the timer resolution.

## Regression budget

WP0 did not leave a checked-in baseline. There is no artifact from before the cutover that shows these caps being chosen against the old cache. The numbers below are a regression budget for the shipping tree, written down on 2026-10-07 after that tree existed. `impl_plan.md` records that missing pre-cutover file as an evidence limit. A later run can reproduce the shipping tree. It cannot turn these caps into a pre-cutover Go/No-Go.

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

The split stays in the tree because the work package was requested after the measurement gate. This file does not claim a latency win for it. `impl_plan.md` now treats that split as part of this branch, not as a package that is still waiting on a later speedup.

## Workload added 2026-10-11

Same machine and `release` profile. Commands are unchanged. This run also prints stage demand and the `syl_query` repeat.

`stages` times the first demand after `snapshot()` on the two-cell fixture: file AST, HIR, TIR, then HWIR. Counts are how many times `parsed_file`, `owner_hir`, `owner_tir`, and `workspace_elaboration` ran during that demand, from the last of the seven iterations. A 0 µs sample is under the timer resolution.

`repeat_query_us` is 21 calls on one snapshot through `hover_at_with_token`, `definition_at_with_token`, `completions_at_with_token`, and `document_diagnostics_with_token`. The older `repeat_ide_us` field is still the session facade (HIR, TIR, package stage diagnostics, and AST). Both are memo hits at this resolution.

```text
SYL_PERF_SESSION {"profile":"release","cold_pair_us":{"n":7,"p50":308,"p95":1208},"repeat_ide_us":{"n":21,"p50":1,"p95":6},"body_edit_us":{"n":7,"p50":144,"p95":156},"signature_edit_us":{"n":7,"p50":153,"p95":162},"cold_mvp_hwir_us":{"n":7,"p50":8493,"p95":12664},"body_edit_modules":1,"body_edit_designs":1,"signature_edit_modules":1,"signature_edit_designs":1,"four_snapshot_rss_kb":32772,"four_snapshot_hwm_kb":32772,"stages":{"n":7,"parse_us":{"n":7,"p50":0,"p95":0},"hir_us":{"n":7,"p50":37,"p95":37},"tir_us":{"n":7,"p50":9,"p95":9},"elab_us":{"n":7,"p50":119,"p95":122},"parse_count":2,"hir_count":3,"tir_count":3,"elab_count":1}}
SYL_PERF_EMIT {"profile":"release","cold_mvp_emit_us":{"n":7,"p50":8116,"p95":14344},"repeat_query_us":{"n":21,"p50":2,"p95":3}}
```

Cold MVP HWIR, cold MVP emit, and the four snapshots in this run stay inside the caps above. `n=7` still makes p95 one sample from the maximum.
