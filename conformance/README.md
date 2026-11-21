# Syl Conformance Suite

The conformance suite is intentionally file-based so external contributors can add cases without
editing harness logic.

Run all partitions from the repository root:

```sh
cargo test -p sylc --test conformance_test
```

[`conformance_test.rs`](../crates/sylc/tests/conformance_test.rs) discovers and sorts
fixtures in each directory. Every Syl file is a standalone compilation unit; it
does not implicitly import adjacent fixtures. Positive cases must complete the
named stage without diagnostics. Negative cases must reach that stage before
failing, so a semantic error cannot accidentally satisfy an elaboration test.

| Partition | Positive coverage | Negative coverage |
| --- | --- | --- |
| `parse` | Constants/functions, packed bundles, interface views, enum matches, generics, place/inplace, registers/select | Missing expressions, generic kinds, match arrows and view fields; unclosed declarations |
| `sema` | Bundle maps, directional views, enum flags, generic defaults, const function widths | Unknown types/arguments, duplicate discriminants, missing aggregate fields, input writes, impure maps, hardware conditions in static `if` |
| `elab` | Disjoint bit/array drivers, guarded drivers, const evaluation and replication, place/inplace, clocked registers | Duplicate drivers, whole/bit and array/bit overlap, partial outputs, undriven reads/signals, conflicting instances and next-state drivers |
| `backend` | Exact SV snapshots for hierarchy, generics/inlining, enum maps, bundle layout, views, registers, replication | Invalid HWIR module/binding/reference/instance names, multiple diagnostics, unsupported SV function calls |

The suite also checks example/std-user compatibility and lossless parser
round-tripping. Functional simulation is a separate integration test:

```sh
cargo test -p sylc --test verilator_sim_test -- --nocapture
```

It invokes the actual `sylc` binary on the MVP ALU and counter examples, builds
C++ testbenches with `verilator --cc --exe --build`, and runs them. The counter
uses a four-bit parameter override to exercise wraparound. Missing Verilator or
C++ compilers produce an explicit `SKIP` message; compilation or simulation
failures fail the test. These tests also run in `scripts/quality_gate.sh`.

## Adding cases

Add a descriptive `name.syl` under the appropriate stage and polarity. For a
negative case, add `name.codes` with one diagnostic code per line, in reporting
order. The harness compares the entire sequence, including repeated codes, and
rejects empty expectations. Diagnostic display text is deliberately not matched.
Use the smallest program that isolates the intended rule, and include a positive
counterpart when checking boundaries such as disjoint versus overlapping bits.

Backend positive cases additionally require `name.sv`. Produce an initial
snapshot with the CLI, then inspect its ports, widths, assignments and clocked
behavior before accepting it:

```sh
cargo run -p sylc -- --out conformance/backend/positive/name.sv conformance/backend/positive/name.syl
```

Snapshots are compared byte for byte and never rewritten by the test runner.

## Backend negative fixtures

The frontend rejects malformed hardware before emission. To test the backend's
own validation boundary, negative cases use a small `.hwir` fixture format
instead of Syl source. Each fixture constructs HWIR directly and passes it to
`SystemVerilogBackend::emit`; it has a sibling `.codes` expectation as above.

The format supports these whitespace-separated directives, one per line:

| Directive | Meaning |
| --- | --- |
| `module NAME` / `end` | Begin/end a module, including an empty one |
| `input WIDTH NAME` / `output WIDTH NAME` | Add a port; width is a single token |
| `drive LHS RHS` | Continuous assignment between identifiers |
| `call LHS FUNCTION` | Drive an identifier from a zero-argument function call |
| `instance TARGET NAME` | Instantiate a target module without connections |

Blank lines and lines starting with `#` are ignored. Unknown directives, missing
module boundaries, and empty designs are fixture errors. For example:

```text
module Top
output 8 y
drive y missing
end
```

This expects `E_HW_UNKNOWN_REFERENCE`. HWIR and SV errors expose structured enum
variants rather than frontend `Diagnostic` codes. The fixture adapter in
[`conformance_backend_test.rs`](../crates/sylc/tests/support/conformance_backend_test.rs)
maps those variants to suite-owned `E_HW_*` / `E_SV_*` identifiers; an unmapped
variant fails the test. These identifiers are fixture contracts, not new public
compiler diagnostic APIs. New cases using existing directives and variants need
no Rust edits.
