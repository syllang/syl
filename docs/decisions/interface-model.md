# Interface Model

Interfaces are currently pure protocol definitions with named views — a named bundle with
directional port subsets:

```syl
interface Stream<T> {
    payload: T
    valid:    Bit
    ready:    Bit

    view source { out payload, out valid, in  ready }
    view sink  { in  payload, in  valid, out ready }
}
```

They are used exclusively through view-select type expressions:

```syl
signal upstream: Stream<Bit>.source
```

The compiler expands the view into individual field-level signals with the correct directions.
There is no `impl` keyword, no `implements` relationship, and no structural subtype check.

## Known Gaps

- **No `impl` declaration.** There is no syntax to declare that a bundle or module satisfies
  an interface. Compatibility relies on manual wiring.
- **No generic constraint `where T: SomeInterface`.** Generic parameters cannot express
  "this type must have these fields."
- **No structural automatch (duck typing).** If two types have identical fields, the compiler
  does not automatically treat them as compatible for an interface.
- **No super-interface or interface inheritance.** Each interface stands alone.

These are deliberate design scope for v1. Closing them would require a trait-like or
structural-subtyping extension to the type system.
