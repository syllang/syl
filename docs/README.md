# Syl Project Policy

## MSRV

Syl uses Rust 1.85 as the minimum supported Rust version. The policy is declared in
`workspace.package.rust-version` and pinned for contributors by `rust-toolchain.toml`.

## Feature Policy

Default workspace builds must compile the parser, semantic layer, elaboration, backend, session,
query, LSP, CLI, fuzz smoke harness, and quality-gate tests. New optional features must document:

- Intended consumer.
- Whether the feature is enabled by default.
- Semver compatibility impact.
- Required test coverage when the feature is enabled and disabled.

## Release Metadata

Publishable crates must keep package description, license, readme, repository or documentation URL,
and workspace MSRV metadata. Private quality crates must set `publish = false`.

## Design Notes

- [Technical debt notes](technical-debt.md)
- [Cell support plan](cell-support-plan.md)
- [Assignment and register update cleanup](assignment-syntax-cleanup.md)
