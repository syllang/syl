# Unsupported Lowering Paths

EIR still contains several explicit unsupported-expression paths. These are older than the
extension-method work, but they show that elaboration and backend coverage is not yet complete. New
features should avoid expanding this unsupported surface without a clear diagnostic and test.

## EirExpr::Unsupported

The `EirExpr::Unsupported { message }` variant acts as a poison token, produced at
approximately **35 sites** across `eir_build.rs`, `eir_map.rs`, `eir_value.rs`,
`eir_body.rs`, `eir_type.rs`, and `eir/facts.rs`.

Downstream passes handle it in various ways:

| Pass | Behaviour |
|------|-----------|
| `eir/validate.rs` | Returns `Err(CompileError::lowering_at(…))` |
| `hw_lower.rs` | Returns `Err(CompileError::lowering_at(…))` |
| `const_mir/lower.rs` | Returns `Err(self.invalid(expr))` |
| `driver_place.rs` | Returns `Err(DriverExprError)` |
| `eir_read.rs` | No-op (skipped in read-place collection) |
| `eir/facts.rs` | No-op (skipped as leaf in fact collection) |
| `driver/tristate.rs` | Assumed `DriveActivity::Active(None)` |
| `driver_place/bounds.rs` | Returns `None` (no bound info) |
