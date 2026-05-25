# Technical Debt Notes

This document records known incomplete areas that should be treated as follow-up work, not as
stable language or compiler design.

## Extension Methods

The current extension-method implementation is a v1 path for cleaner user code:

```syl
map fire<T>(this stage: Stage<T>.tap) -> Bit = ...

signal active: Bit := stage.fire()
```

The supported path is intentionally narrow:

- Extension `map` dot calls compile through semantic analysis, EIR, and backend lowering.
- Extension `fn` dot calls are wired in const MIR lowering, but still need an integration test.
- `cell` and `module` receivers are intentionally rejected for now.
- Receiver generic inference covers the current `Stage<T>.tap` style use case, but is not a
  complete generic constraint solver.
- Directional receivers such as `this: in`, `this: out`, or `this: inout` are not modeled yet.

## Method Lookup

Method lookup currently works, but the shape is not the desired long-term architecture:

- Visibility is based on type package and import-path checks.
- Ambiguous-method diagnostics exist, but need dedicated regression tests.
- Extension method tables are exposed from `HirDesign`; this should eventually move behind a
  dedicated resolver/query boundary.

## EIR Method Lowering

EIR lowering supports simple local receiver calls such as:

```syl
stage.fire()
```

It does not yet support complex receiver expressions such as:

```syl
(foo.bar).method()
array[i].method()
```

Read-place facts for extension calls are also incomplete. The current implementation avoids fake
reads of the callee or receiver binding, but it does not yet preserve field-level reads inside an
extension map with the same precision as direct source expressions.

## Test Gaps

The following tests should be added before treating extension methods as stable:

- Extension `fn` integration.
- Ambiguous method diagnostic.
- Unknown method diagnostic.
- Import and visibility behavior.
- Bundle and interface receiver coverage beyond the current `Stage<Bit>.tap` test.
- Parser negative tests for non-leading `this`.
- Parser negative tests for `this` combined with port directions.

## Public API Shape

`Param::receiver` and `HirSignatureParam::receiver` are currently boolean markers. That is
acceptable for the current syntax, but if receiver metadata grows to include direction, capability,
or mutability, this should become a dedicated receiver model instead of accumulating boolean fields.

## Existing Unsupported Lowering

EIR still contains several explicit unsupported-expression paths. Those are older than the
extension-method work, but they show that elaboration and backend coverage is not yet complete. New
features should avoid expanding this unsupported surface without a clear diagnostic and test.
