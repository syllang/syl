# Query-driven cycle rules

These are the language and query rules the cutover tests lock. A Salsa query cycle is not a Syl diagnostic. Salsa cycle recovery does not decide a language result.

The session evidence is `language_recursion_returns_deterministic_diagnostics` and `salsa_query_cycle_is_not_a_language_diagnostic` in `crates/syl_session/src/cutover_tests/cycle_tests.rs`. The same snapshot produces the report twice. Spans are `source:start..end` byte offsets in that snapshot's source map.

The stage order in this document is the display order of the cycle report in `report` (`crates/syl_session/src/cutover_tests/cycle_tests.rs`). That report asks parse, HIR, TIR, and semantic separately and prints those sections in that order. Inside one section the messages stay in the order that call returns them. A section with one diagnostic has no second message to sort.

That display order is not a merged production diagnostic list. `semantic_diagnostics` returns the first non-empty stage among HIR, TIR, and elaboration (`diagnostics_from` in `crates/syl_session/src/query/lower.rs`). It does not concatenate those stages, and it does not include parse diagnostics. When more than one package shard is present, the semantic section is the concatenation of those per-shard results, so one import error can appear once per shard. The IDE grouped diagnostic API keeps parse, HIR, and TIR in separate stage groups and leaves the elaboration group empty. It does not build one combined list.

The missing-import fixture shows the report: HIR reports `E_MIDDLE_UNKNOWN_IMPORT` and then `E_MIDDLE_UNRESOLVED_NAME`; TIR reports the unresolved name; each semantic shard repeats that HIR pair and does not append the TIR diagnostic again.

## Import and name graph

Syl has no visibility modifier. A name is visible when it is declared in the module or imported from a module that declares it. A cycle in the import graph is allowed.

| case | allowed | result | owner |
| --- | --- | --- | --- |
| mutual `use` of names that exist | yes | no diagnostic; no hardware module when the files contain no cell | `syl_sema` import resolution, reported by `syl_session` |
| `use` of a name the other module does not declare, then a use of that name | no | HIR `E_MIDDLE_UNKNOWN_IMPORT` on the `use` span, then HIR and TIR `E_MIDDLE_UNRESOLVED_NAME` on the use. Semantic repeats that pair once per package shard. No HWIR module | `syl_sema` name resolution |
| const names that import each other and a port width reads one of them | the import is allowed; the value cycle is not a number | no diagnostic code. The port width is `unsupported(const cycle)`. One HWIR module | EIR const inlining in `syl_elab` |

The unknown-import fixture is `use b.missing` at `0:0..13` and `y := missing` at `0:42..49`. The mutual import of existing functions is the `import cycle` case and stays empty.

## Type and generic recursion

A struct or generic struct may name itself in a field. That declaration is not a diagnostic, and a cell that does not read the struct still elaborates. Syl does not require a finite layout until a consumer asks for a nat width or a hardware value. The locked cases are `struct Node { child: Node }` and `struct Rec { child: Box<Rec> }`: no diagnostics, one HWIR module, port width `1`.

## Const and enum recursion

| case | demanded value | result | owner |
| --- | --- | --- | --- |
| `const A = B` and `const B = A` used as `UInt<A>` | a port width | no diagnostic code. Width text is `unsupported(const cycle)`. The module is kept | `EirBuilder::elab_ident` |
| the same const cycle split across two modules | a port width | the same unsupported width. The import cycle is not a second error | same guard, through imported names |
| `enum` discriminants that name each other | a nat discriminant | TIR and semantic `E_MIDDLE_REQUIRES_NAT` on the discriminant expression (`State.Idle` at `0:24..28`). No HWIR module | `syl_sema` enum checking |
| const evaluator step limit | a const MIR value | `E_MIDDLE_CONST_STEP_LIMIT` at 10_000 steps, and the const has no value | `syl_sema` `crates/syl_sema/src/ir/const_mir/eval.rs`, locked by `const_evaluator_reports_structured_step_limit_for_long_running_const_fn` |

The session fixture `const WIDTH = burn(20000)` used as `UInt<WIDTH>` does not run that evaluator. Width lowering leaves the call symbolic (`call(burn,20000)`), with no diagnostic and one HWIR module. The step limit applies when const evaluation is demanded. It is not a width-printing rule.

## Map and function recursion

A map call inlines the map body. A map that reaches its own def while that body is being inlined is not a value: EIR uses `unsupported("map cycle")`. Validation then reports semantic `E_MIDDLE_UNSUPPORTED_HW_VALUE_EXPR` (`expression is not supported by hardware value lowering`) on the assignment and emits no HWIR module. The locked span for `y := ping(x)` is `0:110..122`.

A map that returns its argument is not a cycle. `y := ping(x)` elaborates one module and does not diagnose.

A function call used as a width is not inlined by that width path. `const STEPS = depth(1)` where `depth` calls `depth(0)` stays `call(depth,1)`, with no diagnostic and one HWIR module. Bounded function recursion is a symbolic call there, not a query cycle and not a step-limit diagnostic.

## Query implementation cycle

`query_cycle_probe` calls itself and reads the revision's files. It has no `cycle_fn`. The failure is a Salsa panic whose message contains `dependency graph cycle`. It is not `E_MIDDLE_*` and it is not stored as a language memo. Production language queries do not call themselves to represent the cases above.
