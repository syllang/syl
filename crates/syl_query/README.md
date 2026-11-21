# syl_query

## Responsibilities

`syl_query` owns protocol-neutral read-only queries over a session-owned
analysis snapshot.

## Inputs

- `syl_session::AnalysisSnapshot` and `syl_session::Project`
- stable HIR IDs from `syl_hir` for semantic identity in query results
- semantic facts and HIR/TIR analysis from `syl_sema` reachable through the
  snapshot
- syntax trees and source coordinates used for navigation, grouped diagnostics,
  and completions

## Outputs

- protocol-neutral diagnostics grouped by package, document, and stage
- hover, definition, completion, and document-symbol results
- cancellation-aware query entrypoints over session-owned compiler facts
- read-only access to machine-readable opaque/public summaries already owned by
  the snapshot
- query traits consumed by LSP, tests, and future non-LSP tools

## Allowed Dependencies

- `syl_session`
- `syl_sema`
- `syl_hir`
- `syl_syntax`
- `syl_span`
- `thiserror`

## Forbidden Dependencies

- `syl_elab`
- `syl_hw`
- `syl_emit`
- `syl_lsp`
- `tokio`
- `tower-lsp`
- `url`

## Allowed Responsibilities

- compute editor-facing answers from an existing snapshot
- keep query result DTOs protocol-neutral
- bridge syntax and sema-owned facts into navigation and diagnostics answers
- gate long-running semantic queries with lightweight cancellation tokens

## Forbidden Responsibilities

- owning workspace state or cache invalidation policy
- becoming a shared DTO bucket for unrelated compiler data
- protocol transport, debounce, cancellation scheduling, or UTF-16 adaptation
- triggering backend emission or elaboration-specific mutation

## Public Surface Policy

Public items are limited to query traits and result DTOs that another crate must
consume. Query engines, collectors, and heuristics stay private so callers see a
stable question-and-answer surface instead of internal traversal machinery.
Opaque summary access is intentionally a borrowed snapshot view
(`AnalysisQueries::opaque_summaries()`) rather than a new query-owned DTO.

`grouped_diagnostics[_with_token]()` contains only parse, HIR, and TIR stages.
`background_elaboration_diagnostics[_with_token]()` is an explicit, separate
hardware DRC channel. It delegates execution to the snapshot per package,
preserves related locations and overlay versions, and observes cancellation
between packages. Neither ordinary diagnostics nor navigation trigger it.

## Core Editor Queries

`EditorQueries` adds references, prepare/rename, workspace symbols, inlay hints,
semantic tokens, and signature help on `AnalysisSnapshot`. Every operation has a
cancellable variant; positions, locations, hints, and edits use UTF-16 coordinates.
The query result types carry no transport dependencies.

References and rename use semantic identity and exact lexical identifier spans.
Body-local symbols stay within their package; definitions and callable parameters
include importing packages and named arguments. Rename rejects reserved identifiers
and binding collisions and returns sorted edits without applying them.

Workspace symbols scan recovered syntax and rank exact, prefix, substring, and
subsequence matches. Inlay hints expose known inferred types, aggregate/enum/array
widths, constant word widths, and clock/reset domains. When a local type is absent
from the type table, only known initializer facts or literal types are displayed.
Semantic tokens combine syntax and HIR classifications into single-line ranges.
Signature help handles named arguments, extension receivers, and incomplete calls.
These queries do not request elaboration.
