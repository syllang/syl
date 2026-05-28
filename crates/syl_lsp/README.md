# syl_lsp

## Responsibilities

`syl_lsp` adapts Syl analysis services to the Language Server Protocol.

## Inputs

- LSP requests and notifications carried by `tower-lsp`
- `syl_session::AnalysisHost` snapshots
- protocol-neutral query results from `syl_query`
- source-coordinate data from `syl_span`

## Outputs

- LSP hover, definition, completion, and document-symbol responses
- published diagnostics with UTF-16 ranges derived from grouped query results
- the `syl_lsp` stdio server runner

## Allowed Dependencies

- `syl_query`
- `syl_session`
- `syl_span`
- `tokio`
- `tower-lsp`

## Forbidden Dependencies

- `syl_hir`
- `syl_sema`
- `syl_elab`
- `syl_hw`
- `syl_emit`
- `syl`
- `sylc`

## Allowed Responsibilities

- LSP protocol adaptation
- protocol-only `LspAdapter` mapping over session/query boundaries
- UTF-16 coordinate mapping
- diagnostic publishing, debounce, and stale-generation cancellation
- launching and serving the stdio language server

## Forbidden Responsibilities

- owning compiler semantic algorithms
- duplicating workspace/session state models outside `syl_session`
- defining protocol-neutral query DTOs
- generating hardware or backend text

## Public Surface Policy

Public items should exist only when an embedder or binary launcher must start or
host the server. Request handlers, mapping helpers, and debounce internals stay
private so the public surface remains a thin protocol adapter over
`syl_session` and `syl_query`.
