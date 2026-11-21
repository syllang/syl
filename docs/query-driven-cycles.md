# Query-driven cycle rules

These are the language and query rules the cutover tests lock. A Salsa query cycle is not a Syl diagnostic. Salsa cycle recovery does not decide a language result.

The session evidence is `language_recursion_returns_deterministic_diagnostics` and `salsa_query_cycle_is_not_a_language_diagnostic` in `crates/syl_session/src/cutover_tests/cycle_tests.rs`. The same snapshot produces the report twice. Spans are `source:start..end` byte offsets in that snapshot's source map.

The stage order in this document is the display order of the cycle report in `report` (`crates/syl_session/src/cutover_tests/cycle_tests.rs`). That report asks parse, HIR, TIR, and semantic separately and prints those sections in that order. Inside one section the messages stay in the order that call returns them. A section with one diagnostic has no second message to sort.

That display order is not a merged production diagnostic list. `semantic_diagnostics` returns the first non-empty stage among HIR, TIR, and elaboration (`diagnostics_from` in `crates/syl_session/src/query/lower.rs`). It does not concatenate those stages, and it does not include parse diagnostics. When more than one package shard is present, the semantic section merges those per-shard results deduplicated in first-seen order. The IDE grouped diagnostic API keeps parse, HIR, and TIR in separate stage groups and leaves the elaboration group empty. It does not build one combined list.

The missing-import fixture shows the report: HIR reports `E_MIDDLE_UNKNOWN_IMPORT` and then `E_MIDDLE_UNRESOLVED_NAME`; TIR reports the unresolved name; the semantic section keeps that HIR pair once in first-seen order and does not append the TIR diagnostic again.

## Import and name graph

Syl has no visibility modifier. A name is visible when it is declared in the module or imported from a module that declares it. A cycle in the import graph is allowed.

| case | allowed | result | owner |
| --- | --- | --- | --- |
| mutual `use` of names that exist | yes | no diagnostic; no hardware module when the files contain no cell | `syl_sema` import resolution, reported by `syl_session` |
| `use` of a name the other module does not declare, then a use of that name | no | HIR `E_MIDDLE_UNKNOWN_IMPORT` on the `use` span, then HIR and TIR `E_MIDDLE_UNRESOLVED_NAME` on the use. Semantic reports that HIR pair once (deduplicated across package shards in first-seen order). No HWIR module | `syl_sema` name resolution |
| const names that import each other and a port width reads one of them | the import is allowed; the value cycle is rejected | TIR and semantic `E_MIDDLE_CONST_CYCLE` on each cyclic const initializer. No HWIR module | `syl_sema` declaration checking |

The unknown-import fixture is `use b.missing` at `0:0..13` and `y := missing` at `0:42..49`. The mutual import of existing functions is the `import cycle` case and stays empty.

## Type and generic recursion

Struct and bundle fields must have a finite value layout. Declaration checking traverses fields (including arrays) after substituting generic arguments. Direct recursion, mutual recursion, and instantiated recursion such as `struct Rec { child: Box<Rec> }` produce `E_MIDDLE_RECURSIVE_STRUCT` on the recursive field. Bundle recursion produces `E_MIDDLE_RECURSIVE_BUNDLE`. These errors appear in TIR and semantic diagnostics and prevent HWIR generation, even when the type is unused.

The expansion path records definition IDs and instantiated arguments, with a maximum depth of 64 to bound polymorphic expansion. Finite nested types such as `Box<Box<nat>>` remain valid; an unused generic argument does not itself imply a layout cycle.

## Const and enum recursion

| case | demanded value | result | owner |
| --- | --- | --- | --- |
| `const A = B` and `const B = A` used as `UInt<A>` | declaration or port width | TIR and semantic `E_MIDDLE_CONST_CYCLE` on the cyclic initializers. No HWIR module | `syl_sema` declaration DFS; EIR width validation provides a second guard |
| the same const cycle split across two modules | declaration or port width | the same structured diagnostics, with source spans in both files. The import cycle is not a second error | same DFS, through resolved definition IDs |
| `enum` discriminants that name each other | a nat discriminant | TIR and semantic `E_MIDDLE_REQUIRES_NAT` on the discriminant expression (`State.Idle` at `0:24..28`). No HWIR module | `syl_sema` enum checking |
| const evaluator step limit | a const MIR value | `E_MIDDLE_CONST_STEP_LIMIT` at 10_000 steps, and the const has no value | `syl_sema` `crates/syl_sema/src/ir/const_mir/eval.rs`, locked by `const_evaluator_reports_structured_step_limit_for_long_running_const_fn` |

TIR const evaluation carries a visited definition set and follows each referenced const in its declaring owner. Cycles used in conditions, loop bounds, or struct fields therefore recover without overflowing the evaluator stack. EIR validates port, signal, and storage width expressions recursively; a cycle constructor yields `E_MIDDLE_CONST_CYCLE` at the width's origin before hardware lowering.

The session fixture `const WIDTH = burn(20000)` used as `UInt<WIDTH>` does not run that evaluator. Width lowering leaves the call symbolic (`call(burn,20000)`), with no diagnostic and one HWIR module. The step limit applies when const evaluation is demanded. It is not a width-printing rule.

## Map and function recursion

A map call inlines the map body. A map that reaches its own def while that body is being inlined is not a value: EIR uses `unsupported("map cycle")`. Validation then reports semantic `E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR` (`expression is not supported by hardware value lowering`) on the assignment and emits no HWIR module. The locked span for `y := ping(x)` is `0:110..122`.

A map that returns its argument is not a cycle. `y := ping(x)` elaborates one module and does not diagnose.

A function call used as a width is not inlined by that width path. `const STEPS = depth(1)` where `depth` calls `depth(0)` stays `call(depth,1)`, with no diagnostic and one HWIR module. Bounded function recursion is a symbolic call there, not a query cycle and not a step-limit diagnostic.

Function bodies are checked at const phase, including local types, mutability, calls, conditions, and return paths. Map return values and match/select arms must agree with their declared types, while integer literals and `zero()`/`z()` retain contextual hardware typing. Zero-width signature stubs skip body validation so incremental cross-module signature reads do not create missing-return diagnostics.

## Duplicate module files

Different file URIs must not map to the same module path. Session HIR assembly reports `E_DUPLICATE_MODULE_FILE` for each later URI in lexical order, with related information pointing to the first URI. Recovery selects that first URI deterministically. Both CLI semantic diagnostics and IDE HIR stage groups expose the collision, including modules with no declarations, and HWIR generation stops. Tests reverse root order to verify the same URI selection and message.

## Query implementation cycle

`query_cycle_probe` calls itself and reads the revision's files. It has no `cycle_fn`. The failure is a Salsa panic whose message contains `dependency graph cycle`. It is not `E_MIDDLE_*` and it is not stored as a language memo. Production language queries do not call themselves to represent the cases above.
