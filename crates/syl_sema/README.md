# syl_sema

## Responsibilities

`syl_sema` owns semantic analysis over syntax and HIR: name resolution, type
checking, const evaluation, capability facts, completion facts, TIR side
tables over stable HIR IDs, sema-owned Const MIR for `fn` evaluation, sema-owned
Map IR for pure combinational map semantics, and structured semantic diagnostics.

## Inputs

- syntax trees from `syl_syntax`
- HIR model and stable IDs from `syl_hir`
- source spans and diagnostics infrastructure from `syl_span`

## Outputs

- semantic side tables keyed by HIR entities
- TIR side tables over HIR, plus sema-owned Const MIR and Map IR programs
- typed HIR and TIR analysis objects consumed by `syl_elab`, `syl_session`, and
  `syl_query`
- semantic hover/definition/completion support over HIR and TIR
- structured semantic errors and diagnostics
- explicit Phase 3 fact owners:
  `HirAnalysis::resolution() -> ResolutionTable`,
  `TirAnalysis::facts() -> SemanticFacts`,
  `SemanticOutput::facts() -> Option<&SemanticFacts>`
- sema-owned fact tables inside `SemanticFacts`:
  `ResolutionTable`, `TypeTable`, `CapabilityTable`, `ConstFacts`,
  `LayoutFacts`, and `ProtocolFacts`

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
- compute types, const facts, capability facts, and sema-owned middle IR facts
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
Phase 3 facts are exposed as a read-only facts facade: HIR owns resolution
graph queries, TIR owns typed/capability/const/layout/protocol facts, and
neither API requires elaboration or HW IR construction.
