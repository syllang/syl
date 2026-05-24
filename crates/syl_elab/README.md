# syl_elab

## Responsibilities

`syl_elab` owns the elaboration pipeline that consumes typed semantic analysis
and produces validated hardware graph output.

## Inputs

- `syl_sema::TirAnalysis`
- semantic facts and HIR-owned identities reachable through `syl_sema`
- source spans from `syl_span`
- hardware IR data model from `syl_hw` as the output carrier

## Outputs

- `HardwareCompiler` and elaboration-stage outputs rooted in TIR input
- elaboration diagnostics and driver facts
- `syl_hw::ParametricHwDesign` for backend consumption

## Allowed Dependencies

- normal dependencies: `syl_hir`, `syl_sema`, `syl_hw`, `syl_span`
- test-only parser dependency: `syl_syntax`

## Forbidden Dependencies

- `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`
- `tokio`
- `tower-lsp`
- `url`

## Allowed Responsibilities

- consume `TirAnalysis` and semantic facts
- elaborate cells, modules, maps, and drivers
- validate elaborated structure before backend emission
- lower elaborated results into the backend-neutral HW IR

## Forbidden Responsibilities

- lexing or parsing source text
- owning workspace state, VFS policy, or document lifecycle
- exposing LSP protocol behavior
- printing or validating target-language text

## Public Surface Policy

Public items should expose elaboration-stage boundaries or final HWIR outputs
that other crates must consume, such as `HardwareCompiler` and
`ParametricHwDesign`. HIR/TIR analysis stages, hover/definition helpers, and
session-style orchestration must stay out of this crate's public API.
