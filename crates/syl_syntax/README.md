# syl_syntax

`syl_syntax` owns Syl lexical and syntactic analysis.

It provides the lexer, parser, typed syntax AST, lossless syntax nodes, and
syntax-level recovery diagnostics. Its AST is intentionally syntax-only: it does
not contain resolved names, type information, driver facts, or semantic IDs.

Downstream crates consume this crate when they need parsed source files or
syntax tree structure.
