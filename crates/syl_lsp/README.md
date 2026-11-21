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
- `serde_json` (dynamic watcher registration options)

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
- duplicating compiler workspace/session state outside `syl_session`
- defining protocol-neutral query DTOs
- generating hardware or backend text

## Public Surface Policy

Public items should exist only when an embedder or binary launcher must start or
host the server. Request handlers, mapping helpers, and debounce internals stay
private so the public surface remains a thin protocol adapter over
`syl_session` and `syl_query`.

Initialization discovers the nearest `syl.toml` above the primary workspace root,
including search roots and trusted opaque summaries. Without a manifest, every
workspace folder is a local import root. File notifications reload manifests and
summary files, and refresh `.syl` roots after creates/deletes. Open editor overlays
survive reloads and take precedence over disk changes. Dynamic watchers are
registered when supported; clients may also send static file notifications.

Text synchronization advertises `INCREMENTAL` and applies every change in order
using the session's atomic UTF-16 edit API. Diagnostics use two cancellable lanes:
parse/HIR/TIR runs immediately on open/close or after a 150ms edit debounce, then
hardware DRC runs after another 350ms and a cooperative yield. Both execute on the
blocking pool over the same immutable snapshot. Navigation reuses that snapshot
without re-publishing session inputs or waiting on the host during background DRC.

Each workspace change cancels both diagnostic tokens before new snapshot work.
Generation and document-version checks reject stale results. The publisher merges
fast and DRC layers, clears obsolete diagnostics after edits, removals, or recovery
from manifest errors, and only republishes changed DRC documents.
