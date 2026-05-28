# syl_session

## Responsibilities

`syl_session` owns workspace loading, document lifecycle, import resolution,
workspace/package graph snapshots, incremental analysis cache state, and
orchestration of compiler stages into a snapshot.

## Inputs

- workspace roots, project configuration, and VFS access
- opened or updated source documents and document versions
- parser, semantic, and elaboration stage crates invoked as orchestration steps

## Outputs

- `AnalysisHost`, `ProjectResolver`, and configuration types
- `ResolvedSnapshot`, `AnalysisSnapshot`, `WorkspaceSnapshot`, and `Project`
- source files, session diagnostics, access to semantic analysis, machine-readable
  opaque summaries, cancellation-aware stage access, and final HWIR

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
- own workspace snapshots, source databases, and package graphs
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
`AnalysisSnapshot::opaque_summaries()` and `Project::opaque_summaries()`, while
workspace-level trusted overlays are registered through
`AnalysisHost::set_opaque_summaries()` or `AnalysisHost::register_opaque_summary()`.
That keeps the merged summary surface shared between session/query/LSP readers
and elaboration consumers instead of turning session into a metadata owner.
