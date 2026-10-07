# Salsa preflight record

Retrospective record for the query cutover already on `epic/query-driven`. It does not treat Rust 1.85 as a verified floor.

## Versions

Workspace pin in `Cargo.toml` and the locked crates in `Cargo.lock`:

- `salsa` `=0.28.2`
- `salsa-macro-rules` `=0.28.2`
- `salsa-macros` `0.28.2` (dependency of `salsa` 0.28.2; kept on 0.28.2 because 0.28.3+ is source-incompatible with `salsa-macro-rules` 0.28.2)

Declared floor: `workspace.package.rust-version` is `1.88`. `CONTRIBUTING.md` documents that floor. `rust-toolchain.toml` stays on stable, which is newer than 1.88.

## Toolchain check

`cargo +1.85.0 check --workspace --all-targets` does not succeed. rustc 1.85.0 (4d91de4e4 2025-02-17) exits 101 before compiling:

```text
error: rustc 1.85.0 is not supported by the following packages:
  icu_collections@2.2.0 requires rustc 1.86
  ...
  syl_session@0.1.0 requires rustc 1.88
```

ICU crates in the lockfile require rustc 1.86. Every workspace package requires rustc 1.88 through `rust-version.workspace`.

`cargo +1.88.0 check --workspace --all-targets` succeeds. rustc 1.88.0 (6b00bc388 2025-06-23) finished the dev check with exit 0.

## Behavior already covered by tests

These `syl_session` tests are the evidence for the decisions the preflight was meant to lock. They run the production snapshot and query path.

- Snapshot isolation: `older_snapshot_answers_from_its_own_inputs` in `crates/syl_session/src/cutover_tests.rs`.
- Input refresh: `disk_import_root_config_std_and_documents_refresh_inputs` in the same file.
- Same-key cancellation: `cancelled_owner_does_not_store_while_same_key_waiter_finishes` in `crates/syl_session/src/cutover_tests/cancel_tests.rs`. Cancelling one in-flight `workspace_elaboration` does not store a success memo; the other request for that key finishes, and a later retry reuses the complete result.
- Request isolation without sharing one key claim: `cancelling_one_request_does_not_stop_another` in `crates/syl_session/src/cutover_tests.rs`.
- Language recursion versus a query cycle: `language_recursion_returns_deterministic_diagnostics` and `salsa_query_cycle_is_not_a_language_diagnostic` in `crates/syl_session/src/cutover_tests/cycle_tests.rs`. The probe query calls itself and panics with `dependency graph cycle`.

`AnalysisSnapshot` is `Send`. The Salsa database's thread-local state is `Send` and not `Sync`, so the generation stays behind a `Mutex`. Two queries share one database by cloning it out of that mutex and holding an in-flight read guard. Publish takes the write guard first.
