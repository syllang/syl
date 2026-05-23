# syl_emit

`syl_emit` owns backend emission from Syl hardware graphs to SystemVerilog.

It depends on checked `syl_hw` data and performs backend-local lowering and
structural SystemVerilog validation. It does not depend on HIR, semantic
analysis, or elaboration internals.

Use `SystemVerilogBackend` to emit SystemVerilog from a `syl_hw`
`ParametricHwDesign`.
