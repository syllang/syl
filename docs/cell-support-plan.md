# Cell Support Plan

Cells should be the language feature for reusable structural hardware components. A cell is not a
pure value function and not a top-level-only module; it owns local hardware structure, can be placed
inside modules or other cells, and exposes a typed boundary to callers.

## Current State

The compiler already has a partial cell path:

- Syntax supports `cell ... { ... }`.
- HIR/TIR classify cells alongside modules and extern modules.
- EIR can inline source cells as `EirCellExpansion`.
- Cell placement can bind arguments, result endpoints, and interface views.
- Driver facts can summarize source cell expansions.
- Opaque and precompiled summaries exist for extern and black-box style boundaries.

The current model is useful, but it is still more of an elaboration mechanism than a polished user
abstraction.

## Design Goals

- Cells must improve organization, not become another scripting layer.
- Users should be able to write small, named, reusable hardware components without forcing every
  helper to be top level.
- Source cells should remain analyzable: driver checks, read facts, interface capabilities, and
  domain behavior must survive through placement.
- Opaque/precompiled cells should be usable through trusted summaries instead of requiring source.
- The language should make the difference between pure maps, structural cells, and top-level
  modules obvious.

## Language Model

Keep these roles distinct:

- `map`: pure combinational value expression.
- `fn`: elaboration-time or const computation.
- `cell`: reusable structural hardware component, placeable inside hardware bodies.
- `module`: public hardware boundary intended for backend emission.
- `extern module`: backend-visible black-box instance.

The recommended v1 rule is:

- Source cells may be placed inside `module` and `cell` bodies.
- Source cells lower by expansion, but preserve an expansion frame for diagnostics and summaries.
- Modules remain backend-emitted boundaries.
- Extern modules remain backend-emitted instances.
- Precompiled or opaque cells require summaries before they can participate in driver analysis.

## Organization Features

To make cells actually help users write clean code, add structure around them instead of only adding
more top-level declarations:

- Allow private helper cells in package files, with only explicitly public APIs exported later when a
  visibility model exists.
- Support nested helper definitions inside a cell only after the resolver has a clean ownership
  model. Do not add ad hoc local item lookup first.
- Prefer composition through named cells and interfaces over long top-level scripts.
- Keep cell internals encapsulated: callers interact with ports and result endpoints, not internal
  signal names.

## Semantic Requirements

Cell support should enforce these invariants:

- All cell inputs, outputs, and result endpoints have explicit capability and direction.
- Interface views are expanded consistently at boundaries.
- A cell cannot leave required outputs or result fields undriven.
- A cell cannot create duplicate or overlapping drivers across the caller/callee boundary.
- Reads and writes inside source cells are reported with expansion-aware origins.
- Generic and domain parameters are specialized per placement.
- Recursive or cyclic cell placement is rejected with a structured diagnostic.

## Implementation Phases

1. Stabilize the existing source-cell path.
   Add tests for source cell placement inside modules and cells, result endpoint routing, interface
   view arguments, generic specialization, and expansion-origin diagnostics.

2. Make boundary facts precise.
   Refine read/write/create summaries so driver analysis can reason about cell boundaries without
   losing field-level facts.

3. Add cycle and ownership checks.
   Reject recursive placement and make diagnostics point at the placement chain.

4. Define opaque/precompiled cell behavior.
   Require explicit summaries for cells without source and validate that summaries match declared
   endpoints and capabilities.

5. Improve user organization.
   Add a package visibility story first. Consider nested helper cells only after the resolver can
   represent local item ownership cleanly.

6. Decide backend boundary policy.
   Keep source cells expanded by default. Add a later option for preserving selected cells as backend
   module boundaries only when naming, summaries, and debug metadata are stable.

## Open Questions

- Should source cells always inline, or should users be able to request a preserved backend module?
- Should cells have explicit effect annotations, or are summaries enough for v1?
- Should result endpoints be preferred over `out` ports for reusable pipeline components?
- What is the minimum visibility model needed before adding nested helper cells?
- How should extension methods interact with cells, if at all?
