# syl_sema

`syl_sema` owns Syl semantic analysis.

It lowers syntax into HIR-facing semantic structures, resolves names, checks
types, evaluates compile-time constants, and produces semantic side tables and
diagnostics.

This crate does not build hardware graphs. Hardware expansion, driver analysis,
and graph validation are owned by `syl_elab`.
