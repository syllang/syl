# syl_lsp

`syl_lsp` adapts Syl analysis to the Language Server Protocol.

It owns LSP transport concerns, UTF-16 protocol mapping, diagnostic publication,
debounce, and stale-generation cancellation. It depends on `syl_query` and
`syl_session` instead of reaching directly into compiler-stage internals.

The crate provides the LSP server library and the `syl_lsp` binary target.
