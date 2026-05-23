# sylc

`sylc` is the command-line compiler for Syl.

It loads Syl source files or directories, builds an analysis session, reports
diagnostics, and emits SystemVerilog through `syl_emit`.

This crate is the CLI boundary. Compiler-stage internals are not normal runtime
dependencies of the binary path; remaining white-box test dependencies are
tracked as migration debt in the repository architecture notes.
