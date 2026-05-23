# syl

`syl` is the user-facing facade crate for the Syl hardware description language
toolchain.

It re-exports the stable entry points needed by embedding applications:

- parsing and syntax diagnostics
- source maps and diagnostic data
- workspace/session loading
- protocol-neutral editor queries
- SystemVerilog emission

This crate intentionally owns no compiler stage logic. Work on semantic
analysis, elaboration, hardware graph internals, or protocol adapters should use
the owning `syl_*` crate directly.
