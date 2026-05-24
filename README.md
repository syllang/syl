# Syl

Syl is an experimental hardware description language project focused on static
analyzability.

The project is licensed under the Apache License, Version 2.0. See
[LICENSE](LICENSE).

## Toolchain Policy

- MSRV: Rust 1.85, pinned by `rust-toolchain.toml` and `workspace.package.rust-version`.
- Edition: Rust 2024 for all workspace crates.
- Feature policy: default builds must include the full parser, semantic, elaboration, backend,
  session, query, LSP, CLI, and quality-gate harnesses. New optional features must document
  their consumer, default state, and semver impact before merging.
- Release metadata: every publishable crate must carry description, license, readme, repository
  or documentation URL, and workspace MSRV metadata.

## Quality Gates

Run `scripts/quality_gate.sh` before release work or broad API changes. It is the shared local
and CI entrypoint for formatting, clippy, workspace tests, parser fuzz smoke, example
parse/sema/elab/emit checks, Verilator smoke, documentation syntax checks, and public API surface
review.
