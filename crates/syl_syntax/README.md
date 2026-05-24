# syl_syntax

## Responsibilities

`syl_syntax` owns lexical analysis, parsing, lossless syntax trees, typed syntax
AST nodes, and syntax-level error recovery.

## Inputs

- source text plus source identity from `syl_span`
- parser requests from session, tests, and frontend tools

## Outputs

- lexer tokens
- parser diagnostics and recovery output
- lossless CST structures
- typed syntax AST values such as `AstFile`

## Allowed Dependencies

- `syl_span`

## Forbidden Dependencies

- `syl_hir`
- `syl_sema`
- `syl_elab`
- `syl_hw`
- `syl_emit`
- `syl_session`
- `syl_query`
- `syl_lsp`

## Allowed Responsibilities

- lexer and parser mechanics
- CST and AST data structures
- syntax diagnostics and recovery on incomplete source
- preserving source spans needed by later stages

## Forbidden Responsibilities

- resolved names or import graphs
- type information, const facts, or capability facts
- elaboration, driver analysis, or hardware generation
- workspace loading, caching, or protocol adaptation

## Public Surface Policy

Public items exist because downstream crates must consume parsed syntax without
depending on parser internals. Parser helpers, recovery implementation details,
and tree-building mechanics stay private unless they are required as stable
syntax inputs or outputs.
