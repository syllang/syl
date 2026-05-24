# syl_session

## Responsibilities

`syl_session` owns workspace loading, document lifecycle, import resolution,
incremental analysis cache state, and orchestration of compiler stages into a
snapshot.

## Inputs

- workspace roots, project configuration, and VFS access
- opened or updated source documents and document versions
- parser, semantic, and elaboration stage crates invoked as orchestration steps

## Outputs

- `AnalysisHost`, `ProjectResolver`, and configuration types
- `ResolvedSnapshot`, `AnalysisSnapshot`, and `Project`
- source files, session diagnostics, access to semantic analysis, machine-readable
  opaque summaries, and final HWIR

## Allowed Dependencies

- `syl_syntax`
- `syl_span`
- `syl_sema`
- `syl_elab`
- `syl_hw`
- `thiserror`
- `url`

## Forbidden Dependencies

- `syl_query`
- `syl_lsp`
- `tower-lsp`
- `syl`
- `sylc`

## Allowed Responsibilities

- own workspace/document metadata and VFS boundaries
- cache and reuse semantic and elaboration results
- assemble analysis snapshots that downstream tooling can query
- coordinate stage execution without redefining stage semantics

## Forbidden Responsibilities

- defining semantic facts or elaboration rules
- owning editor query DTOs or query ranking policy
- UTF-16 protocol mapping, diagnostic publish scheduling, or debounce
- backend text emission

## Public Surface Policy

Public items are restricted to workspace/session handles and snapshot access
that CLI, queries, LSP, and embedders must share. Database internals, cache
plumbing, elaboration internals, and orchestration helpers remain private so
`syl_session` exposes state boundaries and sema-owned analysis access, not
implementation details. Opaque summary access stays read-only through
`AnalysisSnapshot::opaque_summaries()` and `Project::opaque_summaries()` rather
than turning session into a metadata owner.
