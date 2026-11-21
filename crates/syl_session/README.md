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
- `syl.toml` manifests and versioned JSON/TOML opaque summary files
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
- `serde`, `serde_json`, `toml`

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

`ProjectManifest::discover_and_load()` searches upward from a file or directory
for `syl.toml`. `load_with_vfs()` and discovery variants support an explicit VFS.
The optional `[project]` table holds `name`, `version`, and `edition`; `[paths]`
holds `workspace_roots`, `std_roots`, `package_roots`, and `opaque_summaries`.
Omitted workspace roots default to the manifest directory; an explicit empty
array stays empty. Paths resolve relative to that directory, canonicalize when
present, and otherwise normalize lexically. Root order is preserved and
duplicates are removed. Unknown fields and blank names/paths are rejected.

`AnalysisHost::from_manifest()` configures roots and loads trusted summaries.
`AnalysisHost::load_manifest_opaque_summaries()` also supports callers that merge
their own root overrides first. The private codec requires `schema_version = 1`
and merges `.json`/`.toml` files in declaration order through the sema-owned
summary builders. It preserves existing host overlays and publishes a revision
only when the merged table changes. Serialization dependencies stay in this
crate; downstream CLI and LSP callers can load manifests without importing sema.
