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
- stable syntax node indices with source ranges for LSP-oriented consumers

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
- preserving lossless trivia in CST form without pushing trivia into semantic crates

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

## Trivia And Span Strategy

- `AstFile` is a typed, lossy AST: comments and whitespace are not stored on AST
  nodes.
- `LosslessSyntaxFile` is the trivia-preserving surface: whitespace and line
  comments are retained verbatim, in order, with exact `Span` values and source
  text slices.
- AST node `Span` values are intended to cover the full concrete construct,
  including names and delimiters that define the node's source extent.
- `AstNodeIndex` derives stable syntax-local node ids from node kind plus the
  covered source text and an occurrence counter. These ids are suitable for
  syntax/LSP bookkeeping, but they are not semantic identities and must not be
  reused as HIR/TIR keys.
