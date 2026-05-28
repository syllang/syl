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
