# syl_emit

## Responsibilities

`syl_emit` owns backend emission from normalized/validated HW IR to
SystemVerilog text and the backend-local SV AST used for emission/debug dumps.

## Inputs

- `syl_hw::ParametricHwDesign` produced by `syl_elab`
- `syl_hw` normalization / validation entry points for backend-independent HW checks

## Outputs

- emitted SystemVerilog source text
- backend-local SV AST / debug dump views
- backend-local structured errors for unsupported HW IR or invalid emitted SV

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
- invoke HWIR normalization before backend lowering
- validate backend-local structural rules
- print SystemVerilog text

## Forbidden Responsibilities

- repair frontend semantic errors
- own backend-independent HW validation rules
- perform name resolution, type checking, or elaboration
- own workspace loading, editor queries, or LSP transport

## Public Surface Policy

Public items exist because CLI, facade, and integration tests need a stable
backend entry point and structured backend errors. Internal SV IR nodes,
validators, and lowering helpers stay private to keep the public surface tied to
HWIR input, backend entry point, and emitted text output only.
