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

## Design Decisions

- [Operator model](decisions/operators.md) — `=`/`:=`/`next` responsibility split
- [Extension methods](decisions/extension-methods.md) — method lookup, EIR lowering, API shape
- [Interface model](decisions/interface-model.md) — named bundle + views, no `impl` yet

## Known Gaps

- [Unsupported lowering paths](gaps/unsupported-lowering.md) — ~35 EIR unsupported-expression sites
- [Test coverage gaps](gaps/test-coverage.md) — extension method, visibility, indexed receiver tests
- [Cell support plan](gaps/cell-support.md) — reusable structural hardware components

## Reference

- [Diagnostic codes](reference/diagnostic-codes.md) — stable error code registry
