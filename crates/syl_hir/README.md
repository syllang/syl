# syl_hir

## Responsibilities

`syl_hir` defines pure HIR data structures and stable IDs shared by semantic,
elaboration, and query code.

## Inputs

- lowered syntax structure produced by `syl_sema`
- source spans and syntax-origin data needed to describe HIR nodes

## Outputs

- stable IDs such as `DefId`, `ExprId`, `LocalId`, and `PackageId`
- HIR item, expression, type, and path data models
- data-only resolution carriers used by semantic side tables

## Allowed Dependencies

- `syl_span`
- `syl_syntax`

## Forbidden Dependencies

- `syl_sema`
- `syl_elab`
- `syl_hw`
- `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`

## Allowed Responsibilities

- define stable IDs and equality domains for HIR-owned data
- provide pure HIR structs and enums
- remain a reusable data model for sema, elab, session, and query layers

## Forbidden Responsibilities

- name resolution algorithms
- type checking, const evaluation, or capability inference
- workspace/session orchestration
- hardware graph construction or backend emission

## Public Surface Policy

Public items are limited to HIR types and IDs that must cross crate boundaries.
Lowering code, checker logic, and caches do not belong here; if an API is only
needed by one implementation crate, it should stay private there instead of
growing the HIR surface.
