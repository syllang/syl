# Diagnostic Code Registry

Diagnostic codes are stable integration points for editor, CLI, and CI consumers. Tests must
assert `Diagnostic.code` for negative parser, semantic, elaboration, and backend paths instead of
matching human-facing `Display` text.

## Syntax

- `E_SYNTAX_PARSE`: parser recovery or expected-token failure.
- `E_SYNTAX_UNEXPECTED_CHARACTER`: lexer rejected a character outside the Syl token set.
- `E_SYNTAX_UNTERMINATED_STRING`: lexer reached EOF before closing a string literal.
- `E_SYNTAX_UNSUPPORTED_STRING_ESCAPE`: lexer rejected a string escape sequence.

## Semantic And Elaboration

Semantic and elaboration diagnostics use the `E_MIDDLE_*` namespace. The registry is implemented
by the structured `code()` methods on `HirError`, `TirError`, `ConstEvalError`,
`CapabilityError`, `EirError`, `DriverError`, and `HwirError`.

New variants in those enums must add a stable code in the same change as the variant and must be
covered by a test that asserts the code value.

## Import Resolution

- `E_IMPORT_RESOLVE`: session import resolver could not resolve an imported path.

## Backend Validation

- `E_HW_DUPLICATE_MODULE`: backend HWIR validation found duplicate module names.
