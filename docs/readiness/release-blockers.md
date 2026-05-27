# Release Blockers

**Status:** Resolved  
**Date:** 2026-05-26  
**Target:** 0.1.0 initial release

---

## Summary

As of 2026-05-26, the critical blockers recorded in the original 2026-02-03 snapshot are resolved.
The previously failing release-gate tests now pass, the MVP example (`comb_alu.syl`) compiles
through the full elaboration and SystemVerilog pipeline, snapshot/LSP regression fixtures match the
current `cell` syntax, and `crates/syl_syntax/src/parser.rs` is back under the 700-line
architecture limit.

This file now records the closure state and the actual root causes that were fixed.

---

## Resolved Critical Items

### 1. MVP example backend failure (`comb_alu.syl`)

**Original symptoms:**

```
regression tests:
  "unknown hardware object Add in module CombAlu"
  "unknown hardware object Xor in module CombAlu"

conformance tests:
  examples/mvp semantic diagnostics: ["E_MIDDLE_UNKNOWN_HW_OBJECT", "E_MIDDLE_UNKNOWN_HW_OBJECT"]
```

**Actual root cause:** `match` shorthand enum patterns like `.Add` and `.Xor` were not being
resolved against the scrutinee enum type during map lowering and cell-body elaboration. They fell
through as unresolved identifiers, which later surfaced as unknown hardware objects.

**Fix:** Lowering now resolves one-segment `match` patterns against the scrutinee enum in both
`MapIrBuilder` and `EirBuilder`, with dedicated regression coverage for map and cell-body matches.

**Release-gate verification:**

| Test file | Test name |
|-----------|-----------|
| `crates/sylc/tests/regression.rs` | `compiles_std_and_mvp_examples` |
| `crates/sylc/tests/regression.rs` | `cli_project_compiles_mvp_examples_from_disk_with_valid_sv_modules` |
| `crates/sylc/tests/conformance.rs` | `conformance_examples_and_std_user_remain_compatible` |
| `crates/sylc/tests/architecture_backend_emit.rs` | `architecture_backend_verilator_smoke_covers_example_and_integration_designs` |
| `crates/syl_sema/tests/const_resolution.rs` | `shorthand_enum_match_patterns_lower_against_scrutinee_type` |
| `crates/syl_sema/tests/const_resolution.rs` | `shorthand_enum_match_patterns_work_in_cell_bodies` |

**Status:** Resolved.

### 2. Session snapshot `hwir()` access regressions

**Original symptoms:**

```
assertion failed: first.hwir().is_some()
assertion failed: base.hwir().is_some()
```

**Actual root cause:** The regression fixtures still used `module ...` even though the frontend now
accepts `cell ...` for hardware callables. The snapshots were never producing valid hardware
semantics, so `hwir()` was absent before cache reuse was even exercised.

**Fix:** Updated valid hardware fixtures in `syl_session` to use `cell`, restoring the intended
snapshot/cache assertions.

**Release-gate verification:**

| Test file | Test name |
|-----------|-----------|
| `crates/syl_session/src/database.rs` | `snapshot_reuses_semantic_cache_for_identical_state` |
| `crates/syl_session/src/database.rs` | `document_scoped_invalidation_preserves_unrelated_snapshot_cache_entries` |
| `crates/syl_session/src/database.rs` | `package_semantic_shards_reuse_unmodified_packages_after_package_edit` |
| `crates/syl_session/src/database.rs` | `workspace_snapshot_tracks_source_database_and_package_graph` |

**Status:** Resolved.

### 3. File size exceeds architecture constraint

**Original symptoms:**

```
architecture_rust_files_stay_under_700_lines ... FAILED
```

**Files over the 700-line limit:**

| File | Lines |
|------|-------|
| `crates/syl_syntax/src/parser.rs` | 715 |
| `crates/sylc/tests/architecture_markers.rs` | 692 (under limit, check precision) |
| `crates/syl_syntax/src/parser/expr.rs` | 692 |
| `crates/syl_hir/src/model/item.rs` | 697 |
| `crates/sylc/tests/driver_overlap.rs` | 691 |
| `crates/syl_syntax/src/ast.rs` | 685 |
| `crates/syl_syntax/src/parser/tests.rs` | 670 |
| `crates/sylc/tests/regression.rs` | 668 |

Only `parser.rs` (715) clearly exceeds 700. Close but over.

**Fix:** Moved `ParseOutput` into `crates/syl_syntax/src/parser/output.rs`, bringing
`parser.rs` back below the 700-line cap without relaxing the rule.

**Release-gate verification:**

| Test file | Test name |
|-----------|-----------|
| `crates/sylc/tests/architecture_markers.rs` | `architecture_rust_files_stay_under_700_lines` |
| `crates/syl_syntax` | full crate test suite |

**Status:** Resolved.

### 4. LSP diagnostic publication: duplicate-driver related locations

**Original symptoms:**

```
diagnostics::tests::publications_preserve_lsp_diagnostic_fields_and_related_locations ... FAILED
"duplicate driver diagnostic must be published"
```

**Actual root cause:** The LSP regression fixture used `module ...` for what was intended to be a
valid duplicate-driver hardware program. Because the input no longer parsed as a hardware cell, the
duplicate-driver diagnostic was never produced.

**Fix:** Updated valid hardware fixtures in `syl_lsp` to use `cell`, restoring duplicate-driver
diagnostic publication and related location coverage.

**Release-gate verification:**

| Test file | Test name |
|-----------|-----------|
| `crates/syl_lsp/src/diagnostics.rs` | `publications_preserve_lsp_diagnostic_fields_and_related_locations` |
| `crates/syl_lsp/src/diagnostics.rs` | `publications_cover_parse_tir_and_query_stage_failures` |

**Status:** Resolved.

---

## Known engineering gaps (should fix, not blocking)

These are tracked in `docs/gaps/`:

| Gap | File | Severity |
|-----|------|----------|
| Import and visibility tests missing | `docs/gaps/test-coverage.md` | Medium |
| Indexed receiver tests missing | `docs/gaps/test-coverage.md` | Low |
| Hierarchical cell boundary precision | `docs/gaps/cell-support.md` | Medium |
| Recursive/cyclic placement diagnostics | `docs/gaps/cell-support.md` | Low |
| Visibility and helper-cell organization | `docs/gaps/cell-support.md` | Low |
| ~35 `EirExpr::Unsupported` sites | `docs/gaps/unsupported-lowering.md` | High |
| CHANGELOG.md is empty | — | Low |
| No doc comment syntax (`///`) | — | Low |

---

## Post-Blocker Checklist

```mermaid
flowchart LR
    A[Critical blockers resolved] --> B[Update changelog\n& release notes]
    B --> C[Final docs/readme sweep]
    C --> D[Tag 0.1.0-alpha]\n\n\n\n\n\n\n\n
```

Critical blocker work is complete. Remaining items in `docs/gaps/` are still worth addressing, but
they are not release-blocking for the 0.1.0 initial release.
