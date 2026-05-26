# Technical Debt Notes

This document records known incomplete areas that should be treated as follow-up work, not as
stable language or compiler design.

## Extension Methods

The current extension-method implementation is a v1 path for cleaner user code:

```syl
map fire<T>(this stage: Stage<T>.tap) -> Bit = ...

signal active: Bit := stage.fire()
```

The supported path is still intentionally narrow:

- Extension `map` dot calls compile through semantic analysis, EIR, and backend lowering.
- Exten  complete generic constraint solver.
- Directional receivers such as `this: in`, `this: out`, or `this: inout` are not modeled yet.
sion `fn` dot calls lower into const MIR and have regression coverage.
- `cell` and `module` receivers are intentionally rejected for now.
- Receiver generic inference covers the current `Stage<T>.tap` style use case, but is not a

## Method Lookup

Method lookup is no longer spread across each call site:

- Visibility is based on type package and import-path checks.
- Unknown-method diagnostics have integration coverage.
- Ambiguous-method candidate selection has resolver-level regression coverage.
- `HirDesign` now exposes an extension-method index model instead of a raw nested table.
- Import and visibility behavior still needs broader integration coverage.

## EIR Method Lowering

EIR lowering supports local and grouped receiver calls such as:

```syl
stage.fire()
(stage).fire()
```

More complex receiver expressions still need dedicated support:

```syl
array[i].method()
```

Read-place facts for map and extension-map calls now use lowered EIR value expressions, so field
reads inside extension methods are recorded as concrete read places instead of fake callee reads.

## Test Gaps

The following tests should be added before treating extension methods as stable:

- Import and visibility behavior.
- Indexed receiver expressions such as `array[i].method()`.
- Full bundle receiver coverage beyond resolver/lowering smoke tests.

## Public API Shape

`Param::receiver` and `HirSignatureParam::receiver` have been replaced with explicit receiver role
models. If receiver metadata grows to include direction, capability, or mutability, extend those role
models rather than adding boolean fields.

## Existing Unsupported Lowering

EIR still contains several explicit unsupported-expression paths. Those are older than the
extension-method work, but they show that elaboration and backend coverage is not yet complete. New
features should avoid expanding this unsupported surface without a clear diagnostic and test.
