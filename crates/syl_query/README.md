# syl_query

`syl_query` defines protocol-neutral query APIs over `syl_session` analysis
snapshots.

It provides diagnostics, hover, definition, completion, and document-symbol
queries without depending on LSP protocol types. This keeps editor semantics
usable by CLI tools, tests, and future non-LSP integrations.

LSP transport, UTF-16 protocol mapping, publish scheduling, debounce, and
cancellation are owned by `syl_lsp`.
