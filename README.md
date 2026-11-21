# Syl

Syl is an early-stage hardware description language focused on static
analyzability. It compiles typed hardware descriptions into SystemVerilog and
shares its analysis pipeline with a Language Server Protocol (LSP) server.
The language and public APIs are still evolving.

## Core features

- Typed combinational and clocked hardware: `Bit`, `UInt<W>`, arrays, bundles,
  enums, registers, and explicit next-state assignments.
- Interfaces with directional views, checked read/write capabilities, and
  field-level connection checking.
- Compile-time functions and constants, generic type/value/domain parameters,
  and static `if`/`for` elaboration.
- Pure combinational `map` expressions, hierarchical `place` instantiation,
  and `inplace` cell expansion.
- Driver checks for conflicting assignments, overlapping bit projections,
  undriven outputs, and uninitialized signal reads.
- SystemVerilog emission, incremental analysis through Salsa, and editor
  diagnostics, hover, definition, completion, and document symbols.

For example, this cell describes an eight-bit combinational incrementer:

```syl
cell Increment(x: in UInt<8>, y: out UInt<8>) {
    y := x + 1
}
```

## Workspace architecture

The table follows dependency order from shared data to application boundaries.
Each crate's README describes its allowed dependencies and responsibilities.

| Layer | Crates | Responsibility |
| --- | --- | --- |
| Source foundation | [`syl_span`](crates/syl_span) | Source identity, spans, coordinate mapping, structured diagnostics |
| Syntax and hardware data | [`syl_syntax`](crates/syl_syntax), [`syl_hw`](crates/syl_hw) | Lexer/parser, lossless CST and AST; independently, backend-neutral HWIR and validation |
| Semantic data and backend | [`syl_hir`](crates/syl_hir), [`syl_emit`](crates/syl_emit) | HIR data and stable IDs over syntax; independently, HWIR to SystemVerilog |
| Semantic analysis | [`syl_sema`](crates/syl_sema) | Resolution, types, capabilities, const evaluation, TIR facts and Map IR |
| Elaboration | [`syl_elab`](crates/syl_elab) | TIR to hardware, cell expansion, driver analysis and HWIR construction |
| Workspace analysis | [`syl_session`](crates/syl_session) | Loading, imports, document snapshots and private Salsa query database |
| Editor queries | [`syl_query`](crates/syl_query) | Protocol-neutral queries over session snapshots |
| Application boundaries | [`syl`](crates/syl), [`sylc`](crates/sylc), [`syl_lsp`](crates/syl_lsp) | Embedder facade; compiler CLI via session/emitter; stdio LSP via session/query |
| Quality tooling | [`syl_fuzz`](crates/syl_fuzz) | Private parser fuzz/smoke harness using syntax and span crates |

The compilation path is syntax → HIR/TIR → elaboration → HWIR → SystemVerilog.
The session coordinates compiler stages; editor queries read session snapshots.
The emitter consumes HWIR without depending on the frontend.

## Quick start

Install Rust via rustup (MSRV 1.88; `rust-toolchain.toml` selects stable).
Run these commands from the repository root:

```sh
cargo build --workspace

# Compile a self-contained design to SystemVerilog.
cargo run -p sylc -- --out target/comb_alu.sv examples/mvp/comb_alu.syl

# Compile a directory, resolving std.* imports from the example library.
cargo run -p sylc -- --std-root examples/std --out target/mvp.sv examples/mvp

# Start the language server on stdin/stdout (normally launched by your editor).
cargo run -p syl_lsp
```

`sylc` accepts one or more files/directories; omit `--out` to write SystemVerilog
to stdout. For editor integration, configure an LSP client to launch the built
`target/debug/syl_lsp` executable with stdio transport.

The full quality gate also requires Python 3, Bash, ripgrep, Verilator, GNU Make,
and a C++ compiler:

```sh
QUALITY_GATE_DIFF_BASE=origin/main bash scripts/quality_gate.sh
```

It checks formatting, clippy, workspace tests (including functional Verilator
simulation when its tools are available), parser fuzz smoke, example compilation
and Verilator lint, documentation syntax, public API snapshots, and Rust docs.
The example smoke gate requires Verilator; it fails if Verilator is absent.

## Examples and conformance

- [`examples/minimal_features.syl`](examples/minimal_features.syl) is an annotated
  language tour.
- [`examples/mvp`](examples/mvp) contains a combinational ALU, clocked counters,
  a buffered stream pipeline, and a replicated lane array.
- [`examples/std`](examples/std), [`examples/std_user`](examples/std_user), and
  [`examples/pipeline_user.syl`](examples/pipeline_user.syl) demonstrate ordinary
  Syl library code and imports.
- [`conformance`](conformance/README.md) contains file-driven parse, semantic,
  elaboration, and backend suites with stable diagnostic expectations and SV
  snapshots.

```sh
cargo test -p sylc --test conformance_test
cargo test -p sylc --test verilator_sim_test -- --nocapture
bash scripts/check_examples.sh
python3 scripts/check_docs_syl.py
```

See [CONTRIBUTING.md](CONTRIBUTING.md) for contribution and commit scope rules.

The project is licensed under the Apache License, Version 2.0. See
[LICENSE](LICENSE).
