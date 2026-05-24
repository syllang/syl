# syl_elab

## Responsibilities

`syl_elab` owns the elaboration pipeline that consumes typed HIR plus semantic
facts and produces validated hardware graph output.

## Inputs

- HIR definitions from `syl_hir`
- semantic stage outputs, typed facts, and diagnostics inputs from `syl_sema`
- source spans and syntax-origin data from `syl_span` and `syl_syntax`
- hardware IR data model from `syl_hw` as the output carrier

## Outputs

- elaboration pipeline stages such as HIR, TIR, and elaboration runners
- elaboration diagnostics and driver facts
- `syl_hw::ParametricHwDesign` for backend consumption

## Allowed Dependencies

- `syl_hir`
- `syl_sema`
- `syl_hw`
- `syl_span`
- `syl_syntax`
- `thiserror`

## Forbidden Dependencies

- `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`
- `tokio`
- `tower-lsp`
- `url`

## Allowed Responsibilities

- consume semantic facts and typed HIR
- elaborate cells, modules, maps, and drivers
- validate elaborated structure before backend emission
- lower elaborated results into the backend-neutral HW IR

## Forbidden Responsibilities

- lexing or parsing source text
- owning workspace state, VFS policy, or document lifecycle
- exposing LSP protocol behavior
- printing or validating target-language text

## Public Surface Policy

Public items should expose stage boundaries or final outputs that other crates
must consume, such as pipeline stages and HWIR results. Internal EIR builders,
driver analyzers, and lowering helpers stay private so the crate exports
contracts between stages instead of leaking pass internals.
