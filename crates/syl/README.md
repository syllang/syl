# syl

## Responsibilities

`syl` is the embedder-facing facade crate. It re-exports the stable entry points
that an application needs to parse source, manage an analysis session, run
editor-neutral queries, and emit SystemVerilog.

## Inputs

- embedder code that constructs parsers, sessions, or emitters
- source text, file metadata, and session requests forwarded to owning crates

## Outputs

- stable facade re-exports for parsing, diagnostics, session loading, query
  access, and SystemVerilog emission

## Allowed Dependencies

- `syl_span`
- `syl_syntax`
- `syl_session`
- `syl_query`
- `syl_emit`

## Forbidden Dependencies

- `syl_hir`
- `syl_sema`
- `syl_elab`
- `syl_hw`
- `syl_lsp`
- `tower-lsp`

## Allowed Responsibilities

- re-export stable entry points for embedders
- keep the top-level user surface small and predictable
- hide compiler-stage crate layout from consumers that do not need internals

## Forbidden Responsibilities

- owning parser, semantic, elaboration, or backend logic
- introducing a second session or query abstraction on top of `syl_session` and
  `syl_query`
- re-exporting compiler internals just because they are convenient for tests

## Public Surface Policy

Everything public in this crate must be public because an embedder needs that
entry point without depending on internal stage crates. `syl` should mostly
re-export stable types from owning crates; if an item is only useful inside one
compiler stage, it stays out of this facade.
