# sylc

## Responsibilities

`sylc` is the command-line boundary for the compiler toolchain.

## Inputs

- CLI arguments and filesystem paths
- workspace roots resolved from the current process environment
- `syl.toml` project manifests and optional trusted opaque summary files
- diagnostics and HW IR obtained through `syl_session` and `syl_emit`

## Outputs

- process exit status
- human-readable or versioned JSON diagnostics on stderr
- emitted SystemVerilog on stdout or a user-selected output path

## Allowed Dependencies

- normal dependencies: `syl_session`, `syl_sim`, `syl_emit`, `syl_span`, `syl_syntax`
- CLI parsing and serialization: `clap`, `serde`, `serde_json`
- test-only white-box dependencies: `syl_elab`, `syl_hw`, `syl_sema`

## Forbidden Dependencies

- normal dependencies on `syl_elab`
- normal dependencies on `syl_hir`
- normal dependencies on `syl_sema`
- normal dependencies on `syl_hw`
- `syl_lsp`
- `tower-lsp`

## Allowed Responsibilities

- parse CLI flags and decide input/output paths
- invoke session loading, diagnostics, and emission in order
- format user-facing command-line diagnostics

## Forbidden Responsibilities

- owning compiler-stage logic
- acting as a library facade
- embedding LSP or editor protocol behavior
- adding runtime dependencies on compiler internals for convenience

## Public Surface Policy

This crate is a binary, so it should not grow a reusable library API unless the
CLI contract itself needs to be embedded. Any new public surface must be justified
as part of the command-line boundary, not as a shortcut around stage crates.

## Usage

```sh
sylc check src/top.syl
sylc emit --manifest syl.toml --out design.sv
sylc --diagnostics-format json check
sylc --out design.sv --std-root ../stdlib src/top.syl
```

Direct invocation without a subcommand retains the legacy emit behavior.
`check` runs parsing, import resolution, semantic checks, and elaboration DRC;
it never emits SystemVerilog and rejects `--out`. `emit` checks first and writes
SystemVerilog to stdout or `--out`. Error diagnostics and project/output errors
exit with status 1; warnings alone do not fail checking.

The shared `--manifest`, repeatable `--std-root` and `--package-root`, and
`--diagnostics-format human|json` flags work before or after either subcommand.
Root flags retain their declaration order even when repeated on both sides of
the subcommand. CLI roots precede manifest roots of the same kind, and duplicate
roots are removed while preserving the first occurrence.

## Native simulation

```sh
sylc sim examples/std/cdc.syl --top Sync2FFAsyncLow --cycles 4
sylc sim examples/std/cdc.syl --top Sync2FFAsyncLow --stimulus steps.json --vcd trace.vcd
```

Each JSON array entry drives a simultaneous group of inputs, settles asynchronous resets and combinational logic, pulses every clock in `clocks` together, then checks `expect`. All three fields are optional. Use flattened top port names, unsigned integers, booleans or binary strings containing 0, 1, X and Z.

```json
[
  {"drive":{"dst_rst_n":0,"async_in":1},"expect":{"sync_out":0}},
  {"drive":{"dst_rst_n":1},"clocks":["dst_clk"],"expect":{"sync_out":0}},
  {"clocks":["dst_clk"],"expect":{"sync_out":1}}
]
```

Automatic runs initialize inputs to zero, assert top resets according to polarity on the first cycle and pulse all top clocks together. `--cycles` includes the reset cycle and defaults to one. A repeated `--param K=V` binds a natural number or boolean. `--strict-unique` fails on overlapping true guards. `--fail-on-x` fails when a top output contains X or Z at the end of a step. The command prints the cycle count and final output bits.

### Deterministic batch simulation

```sh
sylc sim examples/benchmarks/wide_alu_array.syl --top WideAluArray --batch 256 --cycles 1000 --seed 42
sylc sim examples/benchmarks/fault_hunt_pipeline.syl --top FaultHuntPipeline --backend cpu-simd --threads 4 --batch 7 --cycles 12 --seed 2691343690239311708 --replay-vcd fault.vcd
```

Batch runs print the selected backend, batch size, cycle count, seed and final
64-bit digest. The default `--backend auto` routes deterministic runs using
the measured CPU/GPU crossover, available device, instance-family shape and
compiler cache. Use `--backend cpu-ref` for a two-state reference comparison,
or `--backend gpu-cuda --gpu-mode batch|family` to select a CUDA kernel.
`--gpu-cache-dir` overrides `SYL_CUDA_CACHE_DIR`. The cache defaults to
`$XDG_CACHE_HOME/syl/cuda` or the system temporary directory's
`syl-cuda-cache`. Use `--no-gpu-cache` for an isolated cold run. The optional
`SYL_CUDA_ARCH` selects an nvcc target such as `sm_86` or `native`.

An invocation using only the original single-trace options retains four-state
simulation and its cycle-count/output display. Direct stimulus and VCD tracing
require a single reference trajectory. Seeded runs and failure replay use deterministic
batch inputs, including with `--strict-unique` or `--fail-on-x`. A failed batch reports
the failing lane, global stimulus step, hierarchy and assertion source span. With
`--replay-vcd` it also writes that trajectory's waveform.

## Project manifests

```toml
[project]
name = "example"
version = "0.1.0"
edition = "2026"

[paths]
workspace_roots = ["src"]
std_roots = ["../stdlib"]
package_roots = ["../packages"]
opaque_summaries = ["vendor.json"]
```

Both tables are optional. Omitted workspace roots default to the manifest's
directory; explicit `[]` stays empty. Other lists default to empty. Paths resolve
relative to the manifest directory, with existing paths canonicalized and absent
paths normalized lexically. Unknown keys, blank names, and blank paths fail with
structured project errors. The version and edition are informational in v1.

`--manifest` loads that file directly. Otherwise, discovery walks upward from
each input, or from the current directory when no inputs are supplied. Inputs
that discover different manifests require an explicit `--manifest`. If exactly
one manifest is found, it configures the invocation, including inputs that have
no manifest. Without a manifest, the current directory is the workspace root.
Without positional inputs, the manifest's workspace roots become inputs; absent
or empty roots produce a clear error.

Opaque summaries use `.json` or `.toml` with `schema_version = 1` and a `summaries`
array. They load through `AnalysisHost::load_manifest_opaque_summaries()` so the
CLI does not own their codec or depend on semantic internals. For example:

```json
{
  "schema_version": 1,
  "summaries": [{
    "kind": "precompiled_cell",
    "callable": "VendorLatch",
    "driven_fields": ["y"],
    "latency_class": "sequential",
    "trust_boundary": "trusted_precompiled"
  }]
}
```

## Diagnostics

Human diagnostics include codes, file/line/column locations, source snippets,
carets, and related notes with tilde underlines. Tabs expand to four spaces;
columns count Unicode scalars. Unknown sources retain the message without a
fabricated location. Output is deterministic and has no ANSI color sequences.

JSON mode writes exactly one document to stderr, including on successful checks
and on project or emission errors. Stdout remains reserved for SystemVerilog:

```json
{"schema_version":1,"diagnostics":[]}
```

Each diagnostic has `severity`, nullable `code` and `source`, `message`, nullable
`location`, and a `related` array. Severity is `error`, `warning`, `information`,
or `hint`. A location contains `file`, `uri`, raw zero-based `byte_start` and
`byte_end`, and one-based `start_line`, `start_column`, `end_line`, `end_column`.
Related entries contain `message` and nullable `location`. Project errors use
`E_MANIFEST_PARSE`, `E_MANIFEST_INVALID`, `E_OPAQUE_SUMMARY_PARSE`,
`E_OPAQUE_SUMMARY_INVALID`, or `E_PROJECT`, with source `syl_session` and no
location. CLI normalization, emission, and output errors use `E_CLI`, `E_EMIT`,
and `E_IO` respectively. Clap usage/help errors retain clap's standard output.
