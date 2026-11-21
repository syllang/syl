# syl_session

## Responsibilities

`syl_session` owns workspace loading, document lifecycle, import resolution,
workspace/package graph snapshots, and the private Salsa database that memoizes
compiler queries. `WorkspacePackage` is a workspace-graph node keyed by module
path. It is not a Rust package and it is not a semantic compilation unit.

The session model follows explicit object ownership:

- `AnalysisHost` is the application service and public command boundary.
- `AnalysisDatabase` is the aggregate root for publication and query ownership.
- `SessionState` owns mutable inputs, document overlays, opaque summaries, and
  the revision clock.
- `ProjectResolver` is the infrastructure/domain service for VFS and import
  policy; `SnapshotAssembler` owns one snapshot build lifecycle.
- `ModulePath` is the value object that constructs and validates module
  identity; `PackageClosure` is the package-graph domain service for transitive
  semantic dependencies.
- `AnalysisSnapshot`, `WorkspaceSnapshot`, `PackageGraph`, and their child
  records are immutable read models. Salsa tracked functions remain thin
  framework adapters because Salsa requires their function shape.

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
- `salsa`
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
- memoize semantic and elaboration results in Salsa, at declaration and owner grain
- assemble analysis snapshots that downstream tooling can query
- coordinate stage execution without redefining stage semantics

## Forbidden Responsibilities

- defining semantic facts or elaboration rules
- owning editor query DTOs or query ranking policy
- UTF-16 protocol mapping, diagnostic publish scheduling, or debounce
- backend text emission

## Public Surface Policy

Public items are restricted to workspace/session handles and snapshot access
that CLI, queries, LSP, and embedders must share. The Salsa database, tracked
ids, and orchestration helpers stay private to this crate. `syl_session`
exposes state boundaries and sema-owned analysis access, not implementation
details. Opaque summary access stays read-only through
`AnalysisSnapshot::opaque_summaries()` and `Project::opaque_summaries()`, while
workspace-level trusted overlays are registered through
`AnalysisHost::set_opaque_summaries()` or `AnalysisHost::register_opaque_summary()`.
That keeps the merged summary surface shared between session/query/LSP readers
and elaboration consumers instead of turning session into a metadata owner.
