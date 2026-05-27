# Doc Comments for Syl

**Status:** Proposed
**Date:** 2026-02-03
**Drivers:** Industrial HDL documentation needs, Verilog export compatibility, minimal syntax complexity

---

## Summary

Adopt Zig-style `///` (item doc) and `//!` (module-level doc) syntax for Syl documentation comments. Each `///` block carries free-form Markdown text, collected into the AST for tool consumption and preserved as `//` comments in Verilog output. No `/* */` block comments, no structured annotation language — companies that need field-level templates write them in Markdown.

---

## Syntax

| Form | Scope |
|------|-------|
| `//!` | Module-level doc. Attaches to the current package/file. |
| `///` | Item-level doc. Attaches to the immediately following declaration. |

Both forms may span multiple consecutive lines. Adjacent `///` lines merge into one document body (same for `//!`). A blank line or a non-doc token terminates the merge.

**Examples:**

```syl
//! AXI4 protocol implementation for the HIP2000 fabric.
//!
//! This package provides the bundle definitions, address decoding
//! maps, and arbitration cells required by the HIP2000 project.
//! See AMBA AXI4 spec (ARM IHI 0022E) for protocol details.

/// Round-robin arbiter with weighted priority per QoS class.
///
/// ## Ports
///
/// | Port      | Direction | Width    | Description                         |
/// |-----------|-----------|----------|-------------------------------------|
/// | `clk`     | in        | 1        | Core clock, rising-edge active      |
/// | `rst_n`   | in        | 1        | Async reset, active-low             |
/// | `req`     | in        | `N`      | Request vector, one-hot per port    |
/// | `grant`   | out       | `N`      | Grant vector, one-hot               |
///
/// ## Arbitration algorithm
///
/// Each port has a 4-bit weight counter. On each arbitration cycle, the
/// port with the highest non-zero weight is selected. All non-zero
/// weights decrement by one per grant; all weights reload when the
/// highest reaches zero.
cell rr_arbiter #(
    N: Nat,
    W: Nat = 4,
)(
    clk: in Clock<D>,
    rst_n: in Reset<D>,
    req: in Bit<N>,
) -> grant: Bit<N>;

/// AXI4 write address channel bundle.
bundle AxAddr<W: Nat, I: Nat> {
    /// Burst address. Must be aligned to burst size boundary.
    addr: UInt<W>,
    /// Burst length - 1. 0 = single-beat, 255 = 256-beat.
    len: UInt<8>,
}
```

---

## Design

### Lexer

A new `DocComment` / `InnerDocComment` variant is added to `LosslessTokenKind`:

```rust
#[non_exhaustive]
pub enum LosslessTokenKind {
    Keyword,
    Ident,
    Int,
    Bool,
    Str,
    Punctuation,
    Whitespace,
    LineComment,           // existing: //
    DocComment,            // new:      ///
    InnerDocComment,       // new:      //!
    Unknown,
}
```

The existing `Lexer::lex_line_comment` method is extended to detect the number of leading `/` characters:

- `// ` + text → `LosslessTokenKind::LineComment` (unchanged)
- `///` + text → `LosslessTokenKind::DocComment`
- `//!` + text → `LosslessTokenKind::InnerDocComment`

The token *text* includes the leading `///` or `//!` prefix, so the lossless tree can round-trip. The AST layer strips the prefix when populating the `doc` field.

The lossless tree preserves all three as distinct token kinds, so formatters and LSP can treat doc comments differently from code comments.

### AST

Every declaration node that can carry documentation gains an optional `doc: Option<String>` field:

```rust
#[non_exhaustive]
pub struct FnItem {
    pub doc: Option<String>,       // collected /// block text, prefix stripped
    pub name: String,
    pub params: Vec<Param>,
    // ...
}

#[non_exhaustive]
pub struct BundleItem {
    pub doc: Option<String>,
    pub name: String,
    pub generics: Vec<GenericParam>,
    pub fields: Vec<FieldDecl>,
    // ...
}

#[non_exhaustive]
pub struct FieldDecl {
    pub doc: Option<String>,
    pub name: String,
    pub ty: TypeExpr,
    // ...
}

#[non_exhaustive]
pub struct Param {
    pub doc: Option<String>,
    pub name: String,
    pub ty: TypeExpr,
    // ...
}
```

Similarly for `PortDecl`, `EnumItem`, `EnumVariant`, `InterfaceItem`, `MapItem`, `CallableItem`, `ConstItem`, `ExternCellItem`, `UseItem`. The full list is in the table below.

### Parser

`parse_attrs` is renamed to `parse_attrs_and_doc` (or a companion `parse_doc_comments` is added). The parser collects consecutive `DocComment` / `InnerDocComment` tokens before an item, strips the `/// ` or `//! ` prefix (plus leading three chars), merges the lines with `\n`, and assigns the result to the item's `doc` field.

**Edge cases:**

- A `//!` appearing *after* the first declaration in a file is a compile error (Zig semantics). Only file-scope `//!` is valid.
- `///` / `//!` interleaved with plain `//` comments: plain comments are collected into the doc block if adjacent, else ignored.
- `///` followed by `@attr(...)`: doc attaches to the item, attrs parse normally, order between them is irrelevant.
- Standalone `///` at end of file: compile error (unattached doc comment).

### HIR and Semantic Analysis

A new small pass (`DocCollector`) runs during or just after parsing:

1. Merges `//!` blocks into a `ModuleDoc` table keyed by source file.
2. Copies `doc` fields from AST nodes into corresponding HIR nodes.
3. Exposes both through `AnalysisQueries`:

```rust
pub trait AnalysisQueries {
    fn doc_for_item(&self, def_id: DefId) -> Option<&str>;
    fn doc_for_field(&self, def_id: DefId, field: &str) -> Option<&str>;
    fn doc_for_module(&self, source_id: SourceId) -> Option<&str>;
}
```

No deep markdown parsing — doc text is stored as-is. Tooling (LSP hover, `syl doc` generator) renders the raw Markdown.

### Verilog / SystemVerilog Export

When Syl code is exported to Verilog, doc comments on ports, parameters, and module declarations are emitted as `//` Verilog comments in the output:

```verilog
// Round-robin arbiter with weighted priority per QoS class.
//
// Ports:
//   clk      in   1    Core clock
//   rst_n    in   1    Async reset
//   req      in   N    Request vector
//   grant    out  N    Grant vector
module rr_arbiter #(
    parameter N = 4,
    parameter W = 4
) (
    input  wire       clk,
    input  wire       rst_n,
    input  wire [N-1:0] req,
    output wire [N-1:0] grant
);
```

This preserves documentation for downstream consumers who work with Verilog only.

---

## Nodes that carry `doc: Option<String>`

| AST Node | Rationale |
|----------|-----------|
| `UseItem` | Rare but possible |
| `ConstItem` | Constant documentation |
| `FnItem` | Function-level description |
| `EnumItem` | Enum semantics |
| `EnumVariant` | Variant-specific docs |
| `BundleItem` | Bundle contract |
| `FieldDecl` | Per-field description (ports, bundle fields) |
| `InterfaceItem` | Interface contract |
| `ViewField` | View capability docs |
| `MapItem` | Combinational map docs |
| `CallableItem` | Cell/module documentation |
| `ExternCellItem` | External module docs |
| `PortDecl` | Module port documentation |
| `Param` | Parameter documentation |
| `GenericParam` | Generic/type parameter docs |
| `ResultBinding` | Module result docs |
| `Attribute` | Attribute-level doc |
| `MatchArm` | Arm-specific intent |
| `SelectArm` | Select arm intent |

---

## Non-goals

- **No `/* */` block comments.** Syl follows Zig's principle of per-line independent tokenization. Multi-line doc is expressed as consecutive `///` lines.
- **No structured annotation syntax** (no `@param`, `@return`, `@see`). Companies that need structured templates express them in Markdown tables and lists. If structured metadata proves essential later, the existing `@attr(...)` syntax should be extended, not a new comment syntax.
- **No markdown rendering in the compiler.** The compiler stores raw text. Rendering is downstream tooling's responsibility (`syl doc`, LSP, etc.).
- **No doc comment on local variables or statements.** Only module-scoped declarations carry doc. Inside function/map bodies, use `//` for inline notes.

---

## Migration from current `//` style

Current Syl examples use `//` for file-level commentary. These will become `//!` or `///` as appropriate. The change is purely lexer-level — examples can be updated mechanically.

The lossless tree already preserves `LineComment` trivia, so a migration script can rewrite `//` commentary that precedes a declaration into `///` without semantic changes.

---

## Tooling

| Tool | Impact |
|------|--------|
| `syl doc` | New subcommand (or `syl_query` extension) that reads `DocCollector` output and renders HTML/Markdown |
| LSP | Hover tooltip shows doc text |
| Formatter | Preserves `///` / `//!` as distinct trivia, aligns prefix |
| Verilog export | Emits doc as `//` comments in generated `.v` |
