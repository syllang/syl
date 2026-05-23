# syl_span

`syl_span` defines source identity, byte spans, UTF-16 positions, source maps,
and diagnostic data structures for Syl.

This crate is the shared source-location layer used by syntax, semantic
analysis, session snapshots, editor queries, and diagnostics.

It does not own diagnostic rendering policy, recovery strategy, HIR IDs,
semantic IDs, or hardware graph IDs. Those concepts belong to the crate that
owns the corresponding compiler stage or arena.
