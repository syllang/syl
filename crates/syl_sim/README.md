# syl_sim

## Responsibilities

`SimDesign::lower` specializes a chosen Syl cell into a word-level design and schedules its combinational assignments. Dense IDs are deterministic within the same top, parameter bindings and elaboration output.

## Inputs

`syl_elab::ElaborationOutput` supplies parametric hardware modules. Pass a map of `ParamValue::Nat` and `ParamValue::Bool` overrides or an empty map for the Syl defaults. Required generics need explicit bindings. Source IDs in spans refer to the input compilation's source database.

## Outputs

The root instance owns the top ports. Every instance points to its canonical module and evaluated bindings through a shared template ID indexing `families`. Members retain independent signal and state IDs. Physical port names and Syl field paths are both available. Signal, register and assertion IDs give direct access to the declaration or assertion span and its elaboration expansion stack.

## Allowed Dependencies

The runtime-independent model consumes `syl_hw`, `syl_elab` and `syl_span`. Integration tests use the parser and semantic frontend.

## Forbidden Dependencies

Execution engines, the SystemVerilog emitter, CLI and IDE services live outside this layer.

## Allowed Responsibilities

Specialization evaluates dimensions, expands generated scopes, connects ports and records explicit memory read/write ports. Tarjan components diagnose feedback across cell boundaries. When whole-word copies introduce apparent feedback, specialization splits transparent copies at the connected slice boundaries and schedules the resulting assignments. For an acyclic design, `comb_eval_order` lists assignment IDs with their dependencies first. Registers and synchronous memory outputs break combinational paths.

## Forbidden Responsibilities

Execution, clock generation, RAM allocation and black-box behavior belong to consumers.

## Public Surface Policy

The concrete arenas, source metadata and lowering entry point are the interface to reference and accelerated executors.

## Information retained during lowering

The existing hardware normalizer checks structure while leaving generated loops, conditions and textual bounds open. EIR knows extern versus defined cells, source-level placement names, port declarations and clock/reset types. The exchange format carries these through to specialization. Per-item origins retain in-place expansion frames. Register declarations and next-state items describe the same state. Assertion triggers encode a reset mask and a negated source condition. Interface ports are split into individual fields; bundle data stays packed.

## Execution contract

- Evaluate unsigned words in the destination context, truncating to the destination width. Concatenations place the first operand at the most significant end. Array index zero selects the least significant element.
- Priority select picks the first true guard in source order. Unique select carries the mutual-exclusion promise; an executor can diagnose overlapping guards. Source order defines the emitted mux chain. With no true guard both forms take the default.
- Zero fills its context. HighZ fills it with Z. Four-state consumers resolve shared nets and retain X for unknown data. A two-state executor must explicitly reject or declare its policy for unsupported X/Z and uninitialized reads.
- Settle combinational logic before sampling rising edges. Coincident clocks sample the same pre-edge snapshot for registers, assertions and RAM ports, then commit together and settle again. Assertions test the positive source condition unless the reset mask is asserted.
- A synchronous reset takes effect on its clock edge. An asynchronous assertion applies immediately and dominates clocked updates while held. Release alone never samples the next value. Polarity is explicit on the bound reset domain.
- Storage and synchronous RAM read outputs start uninitialized. Enabled synchronous reads see the pre-write word on a same-address read/write edge. Disabled ports hold their state. Distributed reads observe committed writes during the next settle. Conflicting simultaneous writes have unspecified data. These contracts follow the standard pulse, skid buffer, arbiter, CDC, RAM and FIFO cells and their C++ reference harnesses.
