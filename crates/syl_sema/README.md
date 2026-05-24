# syl_sema

## Responsibilities

`syl_sema` owns semantic analysis over syntax and HIR: name resolution, type
checking, const evaluation, capability facts, completion facts, and structured
semantic diagnostics.

## Inputs

- syntax trees from `syl_syntax`
- HIR model and stable IDs from `syl_hir`
- source spans and diagnostics infrastructure from `syl_span`

## Outputs

- semantic side tables keyed by HIR entities
- typed HIR and TIR analysis objects consumed by `syl_elab`, `syl_session`, and
  `syl_query`
- semantic hover/definition/completion support over HIR and TIR
- structured semantic errors and diagnostics

## Allowed Dependencies

- normal dependencies: `syl_hir`, `syl_syntax`, `syl_span`, `thiserror`
- test-only white-box dependencies: `syl_elab`, `syl_emit`, `syl_hw`

## Forbidden Dependencies

- normal dependencies on `syl_elab`
- normal dependencies on `syl_hw`
- normal dependencies on `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`

## Allowed Responsibilities

- lower syntax into HIR-owned semantic structures
- resolve names and imports
- compute types, const facts, and capability facts
- expose semantic analysis objects and lookup APIs that later crates can read

## Forbidden Responsibilities

- building hardware graphs
- driver analysis that depends on elaborated structure
- workspace ownership, VFS policy, or LSP protocol mapping
- SystemVerilog emission or backend repair of semantic errors

## Public Surface Policy

Public items must exist because session, query, and elaboration need semantic
facts, typed analysis objects, or structured errors across crate boundaries.
Internal walkers, caches, and checking helpers should remain private so
`syl_sema` exports facts and semantic lookups, not its implementation details.
