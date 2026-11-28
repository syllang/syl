# syl_sim

## Responsibilities

`SimDesign::lower` specializes a chosen Syl cell into a word-level design and schedules its combinational assignments. Dense IDs are deterministic within the same top, parameter bindings and elaboration output.

## Inputs

`syl_elab::ElaborationOutput` supplies parametric hardware modules. Pass a map of `ParamValue::Nat` and `ParamValue::Bool` overrides or an empty map for the Syl defaults. Required generics need explicit bindings. Source IDs in spans refer to the input compilation's source database.

## Outputs

The root instance owns the top ports. Every instance points to its canonical module and evaluated bindings through a shared template ID indexing `families`. Members retain independent signal and state IDs. Physical port names and Syl field paths are both available. Signal, register and assertion IDs give direct access to the declaration or assertion span and its elaboration expansion stack.

## Allowed Dependencies

The concrete model consumes `syl_hw`, `syl_elab` and `syl_span`. The CPU executor uses inline packed words for widths through 64 bits and unsigned multiprecision arithmetic for larger operands. Integration tests use the parser and semantic frontend.

## Forbidden Dependencies

The SystemVerilog emitter, CLI and IDE services live outside this layer.

## Allowed Responsibilities

Specialization evaluates dimensions, expands generated scopes, connects ports and records explicit memory read/write ports. Tarjan components diagnose feedback across cell boundaries. When word-level dependencies introduce apparent feedback, specialization partitions fixed reads, copies and bitwise AND, OR and XOR at connected slice boundaries. Each interval has an executable assignment. Port connections align low bits, discard truncated high bits and zero-extend narrower unsigned words. For an acyclic design, `comb_eval_order` lists assignment IDs with their dependencies first. Registers and synchronous memory outputs break combinational paths.

## Forbidden Responsibilities

Consumers supply input stimuli, clock events and application-specific extern models.

## Public Surface Policy

The concrete arenas, source metadata and lowering entry point are shared with `Simulator` and waveform consumers.

## Information retained during lowering

The existing hardware normalizer checks structure while leaving generated loops, conditions and textual bounds open. EIR knows extern versus defined cells, source-level placement names, port declarations and clock/reset types. The exchange format carries these through to specialization. Per-item origins retain in-place expansion frames. Register declarations and next-state items describe the same state. Assertion triggers encode a reset mask and a negated source condition. Interface ports are split into individual fields; bundle data stays packed.

## Execution contract

- Packed signals are unsigned. Decimal literals have a minimum width of 32 bits and include a positive sign bit. They use signed arithmetic when both operands and the enclosing expression are signed. The destination supplies width, and assignment truncates the result. Concatenations place the first operand at the most significant end. Array index zero selects the least significant element.
- Priority select picks the first true guard in source order. Unique select carries the mutual-exclusion promise; an executor can diagnose overlapping guards. Source order defines the emitted mux chain. An unknown guard merges corresponding arm and fallback bits.
- Zero fills its context. HighZ fills it with Z. Each inout connection shares the overlapping low bits of the formal and the selected actual. Wider formal bits resolve their local drivers. Four-state consumers resolve shared nets and retain X for unknown data. A two-state executor must explicitly reject or declare its policy for unsupported X/Z and uninitialized reads.
- Propagate changed clock pins and their combinational dependencies before sampling. On coincident edges, registers, RAM ports and clocked extern models see the new pin levels and the old storage values. Commit the sampled updates together. A masked assertion fires when the reset is known inactive and the condition is known false.
- A synchronous reset takes effect on its clock edge. An asynchronous assertion samples the reset expression immediately. While reset is active, each rising clock edge samples that expression again. Release alone never samples the next value. An inactive-to-X/Z reset transition triggers the asynchronous process and samples the normal next value. X/Z-to-active samples the reset expression.
- Storage and synchronous RAM read outputs start uninitialized. Enabled synchronous reads see the pre-write word on a same-address read/write edge. Disabled ports hold their state. Distributed reads observe committed writes during the next settle. Conflicting simultaneous writes have unspecified data. These contracts follow the standard pulse, skid buffer, arbiter, CDC, RAM and FIFO cells and their C++ reference harnesses.


## Running the CPU reference model

Create a `Simulator::new(&design)`, drive the top inputs, then call `eval_comb()` to settle that group of drives. Use `tick("clk")` for a pulse or `step_clocks` for simultaneous rising and falling events. A cycle counts a group containing a rising edge. Read a signal by ID, use `read_named` for a physical top port name, or inspect storage with `memory`. `Simulator::with_mode(&design, StateMode::TwoState)` initializes storage to zero and maps X and Z expression results to zero.

Register an `ExternModel` for each external instance before evaluation. Models receive their named inputs on input changes and return all output values. An unregistered instance produces a runtime error naming its Syl path. A failed clocked assertion aborts the edge before sequential writes commit. Set `strict_unique` to turn a unique-overlap diagnostic into an error.

`VcdWriter::sample(time, &sim)` writes changes at the supplied nanosecond timestamp. `VcdWriter::filtered` accepts a predicate over signal IDs and full Syl paths.

## Running independent two-state trajectories

`BatchCpuSimulator` accepts `Value` inputs for independent trajectories.
Signals are stored in slot-major order. Each lane has its own RAM and event
history and uses T2 to settle combinational logic and sample clock groups.
`new` selects two-state mode and `with_mode` also supports four-state values.
A clocked assertion failure carries the global lane index and the one-based
failing edge in both the batch error and its nested assertion diagnostic.

Compile a `FastProgram` from the specialized `SimDesign` and use it to create a
`FastBatchCpuSimulator`. Its `for_each_tile` callback receives a global lane
offset and exclusive access to a `FastTile`. Use the same program throughout.
Fill each input limb with `input_word`, masking unused high bits. Call `tick`
for a rising edge followed by a low settle, then read output limbs with `word`.
`eval_comb` settles input changes without advancing the clock. Each row holds
one little-endian 64-bit limb across all trajectories in the tile.

The fast path initializes storage to zero and projects X/Z values to zero.
It supports a single external clock, synchronous resets, register updates,
RAM words up to 64 bits, fixed packed slices and clocked assertions. Wider
packed buses use multiple limbs. Compilation returns an error for unsupported
operations such as wide division, dynamic packed indexing or external cells.
Wide addition, bitwise logic and equality use little-endian limbs.
On an assertion failure, `tick` returns the tile-local lane and one-based edge
before committing the sequential updates. Add the tile offset to recover the
global trajectory index.

## Single trajectories

For deterministic two-state runs, `JitProgram` compiles the input generator, combinational DAG, edge schedule and cycle loop with Cranelift. A compiled program can run independent trajectories concurrently. Its reusable `JitCpuSimulator` workspace starts each trajectory at zero. Registers cross native loop iterations as SSA block arguments. RAM stays in word-addressed storage. The final low-phase settle returns all output limbs, including at cycle zero.

`FastTile` uses the scalar interpreter for a single lane. Continuous ticks settle once before sampling the edge. A read after the edge materializes the low-phase values and subsequent reads share that settlement. `FastFamilyProgram` uses SIMD rows for independent sibling subtrees with matching concrete programs. Scalar boundary operations pack and unpack parent pins, and parent and member storage sample the same old state.

## Routed batches

`select_backend` uses the explicit GPU availability and cache-hit observations in
`BackendRequest`. `select_local_backend` obtains those observations on the host
and retains the selected program for repeated calls to `LocalBackend::run`.
`run_batch` executes an explicitly resolved backend.

Each lane uses the same SplitMix64 input stream on all backends. The first
edge asserts top resets according to polarity. Packed top ports occupy consecutive
64-bit word indices in declaration order. `batch_stimulus_word` takes the
port's first index and a little-endian limb offset. The final digest is the
wrapping sum of output words multiplied by `2 * (output_limb * batch + lane) + 1`.
At zero cycles each backend settles the initial inputs and computes this digest.
Detection orders failures by global stimulus step, lane and assertion ID, while
execution continues through the requested final cycle. The run verifies that
coordinate with the reference simulator and returns its source location.
`replay_failure` can also record the reference waveform through the failing edge.

CUDA runners compile on demand with nvcc. The cache key covers generated
source, compiler version and target architecture. Forced family mode uses a
single-thread block for a design without independent mappable children.
