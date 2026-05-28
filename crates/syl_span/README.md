# syl_span

## Responsibilities

`syl_span` owns source identity, byte spans, UTF-16 coordinate mapping, source
maps, and cross-crate diagnostic payloads.

## Inputs

- registered source text and source URIs
- byte offsets and spans produced by parser, sema, elab, session, and LSP code

## Outputs

- `SourceId`, `Span`, `SourcePosition`, `SourceRange`
- `SourceFile` and `SourceMap`
- cross-crate `Diagnostic` values and related info

## Allowed Dependencies

- `std` only

## Forbidden Dependencies

- `syl_syntax`
- `syl_hir`
- `syl_sema`
- `syl_elab`
- `syl_hw`
- `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`
- `syl`
- `sylc`

## Allowed Responsibilities

- represent source coordinates and file identity
- convert byte offsets to UTF-16 protocol coordinates
- carry structured diagnostics across crate boundaries

## Forbidden Responsibilities

- parser recovery policy
- HIR IDs, semantic IDs, or hardware IDs
- name resolution, type checking, elaboration, or backend emission
- diagnostic rendering, publishing, debounce, or transport concerns

## Public Surface Policy

Items are public here only when multiple crates must exchange source locations
or diagnostics through a shared type. Stage-local helper functions and policies
stay in the crate that owns that stage, not in `syl_span`.
