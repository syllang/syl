# sylc

## Responsibilities

`sylc` is the command-line boundary for the compiler toolchain.

## Inputs

- CLI arguments and filesystem paths
- workspace roots resolved from the current process environment
- diagnostics and HW IR obtained through `syl_session` and `syl_emit`

## Outputs

- process exit status
- human-readable diagnostics on stderr
- emitted SystemVerilog on stdout or a user-selected output path

## Allowed Dependencies

- normal dependencies: `syl_session`, `syl_emit`, `syl_span`, `syl_syntax`
- test-only white-box dependencies: `syl_elab`, `syl_hw`, `syl_sema`

## Forbidden Dependencies

- normal dependencies on `syl_elab`
- normal dependencies on `syl_hir`
- normal dependencies on `syl_sema`
- normal dependencies on `syl_hw`
- `syl_lsp`
- `tower-lsp`

## Allowed Responsibilities

- parse CLI flags and decide input/output paths
- invoke session loading, diagnostics, and emission in order
- format user-facing command-line diagnostics

## Forbidden Responsibilities

- owning compiler-stage logic
- acting as a library facade
- embedding LSP or editor protocol behavior
- adding runtime dependencies on compiler internals for convenience

## Public Surface Policy

This crate is a binary, so it should not grow a reusable library API unless the
CLI contract itself needs to be embedded. Any new public surface must be justified
as part of the command-line boundary, not as a shortcut around stage crates.
