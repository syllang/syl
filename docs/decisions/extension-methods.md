# Extension Methods

Extension methods provide a v1 path for cleaner user code:

```syl
map fire<T>(this stage: Stage<T>.tap) -> Bit = ...

signal active: Bit := stage.fire()
```

## Supported Path

The supported path is intentionally narrow:

- Extension `map` dot calls compile through semantic analysis, EIR, and backend lowering.
- Extension `fn` dot calls lower into const MIR and have regression coverage.
- Directional receivers such as `this: in`, `this: out`, or `this: inout` are not modeled yet.
- `cell` and `module` receivers are intentionally rejected for now.
- Receiver generic inference is not yet a complete generic constraint solver.
- Receiver generic inference covers the current `Stage<T>.tap` style use case.

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

## Public API Shape

`Param::receiver` and `HirSignatureParam::receiver` have been replaced with explicit receiver role
models. If receiver metadata grows to include direction, capability, or mutability, extend those role
models rather than adding boolean fields.
