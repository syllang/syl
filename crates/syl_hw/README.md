# syl_hw

## Responsibilities

`syl_hw` owns the backend-neutral hardware IR data model produced by elaboration
and consumed by emitters. It is a data contract only: no EIR builder state,
driver-analysis scratch state, or semantic temporaries should leak into it.

## Inputs

- elaborated hardware structure produced by `syl_elab`
- source-origin data from `syl_span` attached to hardware objects

## Outputs

- hardware object IDs
- module, port, instance, connection, guard, place, and expression data
- parametric hardware design containers consumed by backends

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

## Forbidden Responsibilities

- parser or semantic analysis
- elaboration algorithms or driver conflict analysis
- carrying elaboration-time temporary state from `syl_elab`
- target-language validation or printing
- workspace orchestration or protocol adaptation

## Public Surface Policy

Public items are limited to the HW IR types that must cross from elaboration to
emitters and tests. Analysis algorithms belong in `syl_elab` or backend crates;
`syl_hw` should stay a data contract, not a behavior bucket.
