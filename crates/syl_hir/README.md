# syl_hir

`syl_hir` defines the Syl high-level intermediate representation and HIR-owned
stable identifiers.

The crate is a data-model layer. It does not perform name resolution, type
checking, const evaluation, or hardware elaboration. Semantic facts are kept in
side tables produced by `syl_sema` instead of being mutated into HIR nodes.
