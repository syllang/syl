# Public API Consumers

Every change that adds a public item must update `api/public-surface.txt` through
`scripts/check_public_api.py --bless` and explain the intended consumer in the relevant section
below or in a more specific item-level note.

## syl

Stable facade for embedding applications and downstream tools that need parsing, sessions,
queries, diagnostics, and SystemVerilog emission without depending on compiler internals.

## syl_elab

Compiler internals and architecture tests that need explicit elaboration stages, hardware metadata,
and HWIR lowering evidence.

## syl_emit

CLI, facade, and backend tests that emit or validate SystemVerilog from normalized HWIR.

## syl_fuzz

Repository-local quality gates and fuzz/smoke jobs. This crate is private and not published.

## syl_hir

Semantic lowering, query, and session internals that consume stable HIR identifiers and typed HIR
models.

## syl_hw

Elaboration, emission, and backend validation code that consumes the hardware graph model.

## syl_lsp

The language-server binary and protocol adapter tests. Public items are intentionally narrow and
protocol-facing.

## syl_query

Editor-neutral query consumers, LSP adapter code, and integration tests that inspect analysis
snapshots without owning workspace state.

## syl_sema

Elaboration, session, query, and compiler tests that consume semantic facts, TIR, diagnostics, and
opaque summary contracts.

## syl_session

CLI, facade, query, and LSP consumers that need project loading, VFS, package snapshots, and
workspace-scoped analysis.

## syl_span

All compiler layers, CLI, LSP, and embedding consumers that need source identity, ranges, and
diagnostics.

## syl_syntax

Parser, HIR lowering, session, query, LSP, fuzz, and documentation checks that consume typed AST,
lossless syntax, tokens, parser entrypoints, and node indexes.
