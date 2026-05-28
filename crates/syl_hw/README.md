# syl_hw

## Responsibilities

`syl_hw` owns the backend-neutral hardware IR data model produced by elaboration
and consumed by emitters. It also owns lightweight backend-neutral
normalization/validation over HW IR itself. No EIR builder state, driver-analysis
scratch state, or semantic temporaries should leak into it.

## Inputs

- elaborated hardware structure produced by `syl_elab`
- source-origin data from `syl_span` attached to hardware objects

## Outputs

- hardware object IDs
- module, port, instance, connection, guard, place, and expression data
- parametric hardware design containers consumed by backends
- backend-neutral HW normalization / validation reports

## Allowed Dependencies

- `syl_span`

## Forbidden Dependencies

- `syl_syntax`
- `syl_hir`
- `syl_sema`
- `syl_elab`
- `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`

## Allowed Responsibilities

- define HW IR structs, enums, builders, and IDs
- carry source origin and backend-neutral hardware structure as data
- provide a backend-neutral exchange format between elaboration and emission
- validate backend-independent HW naming/reference/interface invariants

## Forbidden Responsibilities

- parser or semantic analysis
- elaboration algorithms or driver conflict analysis
- carrying elaboration-time temporary state from `syl_elab`
- target-language validation or printing
- workspace orchestration or protocol adaptation

## Public Surface Policy

Public items are limited to the HW IR types and validation entry points that
must cross from elaboration to emitters and tests. Elaborator algorithms still
belong in `syl_elab`, and target-language validation still belongs in backend
crates; `syl_hw` should stay a narrow HW data contract plus backend-neutral
checks.
