# Syl Conformance Suite

The conformance suite is intentionally file-based so external contributors can add cases without
editing harness logic.

Current partitions:

- `parse/positive` and `parse/negative`: syntax acceptance and recovery diagnostics.
- `sema/positive` and `sema/negative`: semantic and TIR checks.
- `elab/positive` and `elab/negative`: elaboration and driver diagnostics.
- `backend/positive` and `backend/negative`: SystemVerilog emission and backend validation.

Negative cases use a sibling `.codes` file with stable diagnostic codes. Tests must assert those
codes, not display text.
