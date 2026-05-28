# syl_fuzz

Private parser fuzz and smoke harnesses used by the repository quality gate.

This crate is not published. It exists so ordinary `cargo` commands can build
the parser fuzz entrypoint in CI without requiring a long-running fuzz engine.
