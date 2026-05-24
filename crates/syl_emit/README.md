# syl_emit

## Responsibilities

`syl_emit` owns backend emission from checked HW IR to SystemVerilog text.

## Inputs

- `syl_hw::ParametricHwDesign` produced by `syl_elab`

## Outputs

- emitted SystemVerilog source text
- backend-local structured errors for unsupported or invalid HW IR

## Allowed Dependencies

- `syl_hw`
- `thiserror`

## Forbidden Dependencies

- `syl_syntax`
- `syl_hir`
- `syl_sema`
- `syl_elab`
- `syl_session`
- `syl_query`
- `syl_lsp`
- `tokio`
- `tower-lsp`

## Allowed Responsibilities

- lower HW IR into backend-specific IR
- validate backend-local structural rules
- print SystemVerilog text

## Forbidden Responsibilities

- repair frontend semantic errors
- perform name resolution, type checking, or elaboration
- own workspace loading, editor queries, or LSP transport

## Public Surface Policy

Public items exist because CLI, facade, and integration tests need a stable
backend entry point and structured backend errors. Internal SV IR nodes,
validators, and lowering helpers stay private to keep the public surface tied to
HWIR input and emitted text output only.
