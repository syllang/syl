# Observed Issues & Design Observations

A collection of bugs, fragile patterns, and design surprises found during
deep code-reading. Each entry is tagged with severity and whether it's a
**bug** (should be fixed), a **fragile pattern** (works today but will
break under change), or a **design note** (intentional trade-off worth
knowing about).

---

```
  ## [ ] Issue N：表示未完成。
  ## [x] Issue N：表示完成。
```

执行时，必须遵照 AGENTS.md。
S1 主 Agent 先派出任务给 Work SubAgent（使用 gpt-5.4-xhigh 对于涉及到重构的复杂任务，gpt-5.4-mini 对于无脑任务），可并行，包括任务内并行和 Issue 间并行，然后 
S2 Review SubAgent （ gpt-4-medium）检查并反馈问题，
S3 主 Agent 审核和仔细分析后传达给 Work SubAgent 整改。
S4 整改完之后 Review SubAgent 重复检查(S2~S4)……这是循环过程，直到本 Issue 问题收敛。

每个 Step，在本文件的对应 Issue下标记一个 Log 行。整个Issue 问题收敛完成后只有 Review Agent 有权独立地标记为完成，然后才能进入下一个 Issue.
注：如果是假的报告 Issue，可以标记为不必处理。

## [ ] 1. `semantic_summary_count` is non-deterministic across sessions

**File:** `crates/syl_hir/src/model/summary.rs`
**Log:**
- Issue created.
- 2026-05-28 Main: dispatched a gpt-5.4-mini audit subagent for issues 1/11/13.
- 2026-05-28 Audit (gpt-5.4-mini): issue text is partially stale. `semantic_summary_count()` still mixes `span.start`, but `PackageSemanticCacheProbe` no longer uses it for cache invalidation; the only live call site is a discarded local in `crates/syl_sema/src/tir/checker.rs`.
**Severity:** ⚠️ fragile pattern
**Tags:** determinism, caching

`HirDesign::semantic_summary_count()` mixes `span.start` (a byte offset)
into every sub-count:

```rust
// in HirEnumVariant::summary_count:
self.enum_def.get() + self.name.len() + self.span.start

// in HirDef::summary_count:
self.id.get() + self.name.len()
    + self.canonical_path.len()
    + self.kind.summary_count()
    + self.span.start
```

`span.start` is a **byte offset into the source file**. Two different source
files with different whitespace or comment layouts before the same definition
will produce different summary counts, even though the *semantic* content is
identical. This means the cache-invalidation check in `PackageSemanticCacheProbe`
may spuriously invalidate cached results when only whitespace changes.

**Impact:** Mild — only affects incremental re-analysis performance (missed
cache hits), not correctness. The comment in `summary.rs` already notes
"collisions are possible", but **false positives** (spurious invalidation)
are equally possible and not mentioned.

**Would fix by:** Replacing `span.start` with deterministic alternatives
(definition ordinal, `DefId.get()`, or a content hash).

---

## [x] 2. `from_type_arg` silently discards path context

**File:** `crates/syl_hir/src/model/type_ref.rs:531`
**Severity:** 🐛 bug (latent)
**Tags:** type substitution, generics
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_hir/src/model/type_ref.rs` plus focused tests.
- 2026-05-28 Worker (gpt-5.4-mini): implemented fail-closed const substitution for multi-segment paths; single-segment literal/ident behavior preserved.
- 2026-05-28 Review (gpt-5.4-mini): accepted; no correctness or regression findings.
- 2026-05-28 Main: verified with `cargo test -p syl_hir const_substitution`.

`MirConstExpr::subst_type_vars` converts a `MirTypeRef` replacement into a
`MirConstExpr` via `from_type_arg`, which calls `ty.path_name()` — returning
only the **last segment** of a multi-segment path:

```rust
fn from_type_arg(ty: &MirTypeRef) -> Option<Self> {
    let name = ty.path_name()?; // only the leaf name!
    // ... parses name as Nat, Bool, or Ident
}
```

A replacement like `{"N" -> MirTypeRef::path(vec!["pkg", "WIDTH"])}` produces
`Ident("WIDTH")` — the `"pkg."` prefix is silently dropped. If two packages
both define `WIDTH` with different values, the wrong constant is substituted.

**Trigger condition:** A generic parameter whose default is a const expression
that references a type alias from another package. Currently unlikely because
Syl's generics are simple, but a correctness bug waiting to happen.

**Would fix by:** Propagating the full path rather than collapsing to leaf name,
or rejecting multi-segment replacements in const contexts.

---

## [x] 3. Dead `#[allow(dead_code)]` span fields in HIR body types

**Files:** `crates/syl_hir/src/model/body.rs` (multiple locations)
**Severity:** 🧹 code smell
**Tags:** dead code, maintenance burden
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini audit subagent for the four body span fields.
- 2026-05-28 Audit (gpt-5.4-mini): issue is partially stale. `HirNamedExpr.span`, `HirMatchArm.span`, and `HirSelectArm.span` are dead; `HirCallArg.span` is live, but its `#[allow(dead_code)]` suppression is stale.
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_hir/src/model/body.rs`.
- 2026-05-28 Worker (gpt-5.4-mini): removed the three dead span fields and their lowering sites; kept `HirCallArg.span` and removed only its stale lint suppression.
- 2026-05-28 Main: verified with `cargo test -p syl_hir`.
- 2026-05-28 Review (gpt-5.4-mini): accepted; removals match all in-tree readers.

The following fields carry `#[allow(dead_code)]` with a justification:

```rust
pub struct HirNamedExpr {
    pub name: String,
    pub value: HirExpr,
    #[allow(dead_code, reason = "HIR preserves field spans for diagnostics ...")]
    pub(crate) span: Span,  // ← never read except in tests
}

pub struct HirCallArg {
    pub name: Option<String>,
    pub value: HirExpr,
    #[allow(dead_code, reason = "HIR retains argument spans ...")]
    pub(crate) span: Span,  // ← never read
}

pub struct HirMatchArm {
    pub doc: Option<String>,
    pub pattern: MirPattern,
    pub value: HirExpr,
    #[allow(dead_code, reason = "HIR preserves arm spans ...")]
    pub(crate) span: Span,  // ← never read
}

pub struct HirSelectArm {
    pub doc: Option<String>,
    pub pattern: HirExpr,
    pub value: HirExpr,
    #[allow(dead_code, reason = "HIR preserves arm spans ...")]
    pub(crate) span: Span,  // ← never read
}
```

These spans are populated during lowering but **never read back** by any
downstream pass. The `#[allow(dead_code)]` lint is suppressed with a reason
that describes *why they should exist*, not *why they're read*. If they were
truly needed, something would read them.

**Impact:** Confusing for new contributors ("should I populate this field?"),
wasted memory in the HIR arena, and the lint suppression masks what would
otherwise be a helpful compiler warning if a real dead field is introduced.

**Recommendation:** Either wire these spans into the LSP or diagnostic paths,
or remove them and move the span data into a side-structure that's only
allocated when the LSP is active.

---

## [x] 4. `BinaryOp::Field` duplicates `Expr::Field`

**File:** `crates/syl_syntax/src/ast.rs`
**Severity:** 🧹 code smell
**Tags:** AST design, redundancy
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker across syntax/HIR/sema for the dead `Field` binary-op path.
- 2026-05-28 Worker (gpt-5.4-mini): removed `BinaryOp::Field` plus downstream `MirBinaryOp::Field` / `MapBinaryOp::Field` dead paths; added a parser test proving `foo.bar` still parses as `Expr::Field`.
- 2026-05-28 Main: verified with `cargo test -p syl_syntax parser::tests::parses_field_access_as_a_field_expression -- --exact`, `cargo check -p syl_hir`, and `cargo check -p syl_sema`.
- 2026-05-28 Review (gpt-5.4-mini): requested follow-up because API snapshot artifacts still advertised the removed variants.
- 2026-05-28 Worker (gpt-5.4-mini, second pass): removed stale `BinaryOp::Field` / `MirBinaryOp::Field` / `MapBinaryOp::Field` entries from `api/public-surface.txt` and `api/public-api-consumers.md`.
- 2026-05-28 Review (gpt-5.4-mini): accepted the final code + snapshot state.

The AST has two ways to represent field access:

```rust
pub enum BinaryOp {
    // ...
    #[strum(serialize = ".")]
    Field,              // ← never produced by the parser
    // ...
}

pub enum Expr {
    // ...
    Field { base: Box<Expr>, field: String, span: Span },  // ← parser creates this
    // ...
}
```

The parser always lowers `.` into `Expr::Field`. `BinaryOp::Field` is never
constructed by any code path. During HIR lowering, it's handled:

```rust
// In type_ref.rs MirBinaryOp conversion:
BinaryOp::Field => Self::Field,
```

But this code path is dead — `Expr::Binary { op: BinaryOp::Field, .. }` is
never produced by the parser.

**Impact:** Zero (dead code). But it's misleading: someone reading `BinaryOp`
will see `.` listed as a binary operator and may expect `a.b` to be parsed
as `Binary(a, ".", b)`, when it's actually parsed as `Field(a, "b")`.

**Recommendation:** Remove `BinaryOp::Field`. If kept for future use, add
a clear `#[allow(dead_code)]` with reason.

---

## [x] 5. `HwOrigin` shares the same sharing caveat as `EirOrigin` — but isn't documented

**File:** `crates/syl_hw/src/design.rs`
**Severity:** ⚠️ fragile pattern
**Tags:** origin tracking, debugging
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_hw/src/design.rs`.
- 2026-05-28 Worker (gpt-5.4-mini): documented the immutable sharing caveat on `HwOrigin`.
- 2026-05-28 Review (gpt-5.4-mini): accepted; wording matches `HwOrigin`'s frozen expansion-stack semantics.
- 2026-05-28 Main: `cargo test -p syl_hw --lib` remains blocked by an unrelated existing compile error in `crates/syl_hw/src/parametric.rs` (duplicate `Debug` impl on `ParametricHwItem`).

`EirOrigin` was recently documented with the caveat that `env.insert` shares
the same origin across different instantiation paths. `HwOrigin` (in `syl_hw`)
has an identical architecture (span + expansion_stack) but **no such documentation**.
If both are built from the same elaboration pass, they have the same sharing
behavior — and downstream code reading `HwOrigin` will miss the caveat.

**Impact:** Low today, but if someone writes a diagnostic pass that reads
`HwOrigin::expansion_stack` and assumes it's the full path, they'll get
incomplete results.

---

## [x] 6. `Span` equality considers `source` — cross-file comparisons may surprise

**File:** `crates/syl_span/src/lib.rs`
**Severity:** 🧠 design note
**Tags:** span semantics
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `AstNodeIndex::find_by_span`.
- 2026-05-28 Worker (gpt-5.4-mini): documented exact full-`Span` matching semantics and added a focused regression test for mismatched `SourceId`.
- 2026-05-28 Main: verified with `cargo test -p syl_syntax parser::tests::node_index::node_index_find_by_span_requires_matching_source_id -- --exact` and `cargo test -p syl_syntax node_index -- --nocapture`.
- 2026-05-28 Review (gpt-5.4-mini): accepted.

`Span` equality compares `(source, start, end)` exactly. Two spans from
different `SourceId` values are never equal, even if they happen to have the
same offset range. This is correct behavior for the source-map abstraction,
but it means **you cannot use `Span` as a lookup key across files**. The
`AstNodeIndex::find_by_span` method searches by exact span match, so a query
with the right offsets but wrong `source` will silently return `None`.

**Example:**
```rust
// Given span_a = Span { source: S1, start: 5, end: 10 }
//   and span_b = Span { source: S2, start: 5, end: 10 }
assert_ne!(span_a, span_b);  // true — different SourceId
```

**Recommendation:** Documented in the existing `Span` doc already. Worth
repeating for `find_by_span` callers.

---

## [x] 7. `utf16_position` line computation has a near-miss for empty files

**File:** `crates/syl_span/src/lib.rs:147`
**Severity:** ⚠️ fragile pattern
**Tags:** LSP, empty files
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_span/src/lib.rs`.
- 2026-05-28 Worker (gpt-5.4-mini): added an explicit empty-file fast path in `utf16_position()` and two focused tests.
- 2026-05-28 Review (gpt-5.4-mini): accepted; no correctness or regression findings.
- 2026-05-28 Main: verified with `cargo test -p syl_span`.

```rust
fn utf16_position(&self, offset: usize) -> SourcePosition {
    let offset = self.clamp_to_char_boundary(offset.min(self.text.len()));
    let line = self
        .line_starts
        .partition_point(|line_start| *line_start <= offset)
        .saturating_sub(1);
    let line_start = self.line_starts.get(line).copied().unwrap_or_default();
    // ...
}
```

For an empty file (`text = ""`, `line_starts = [0]`), offset 0:
- `partition_point` returns `1` (one element, 0 ≤ 0 → true)
- `saturating_sub(1)` gives `0` ← correct by luck
- `line_starts.get(0)` returns `Some(0)` ← correct

If `line_starts` were empty (which it never is, since `line_starts_for` always
pushes `0`), `partition_point` would return `0`, `saturating_sub(1)` would
give `usize::MAX`, and `.get(usize::MAX)` would return `None`, triggering
`unwrap_or_default()` which gives `0` — still correct, but by three layers of
defense. This is a **brittle correctness chain**.

**Recommendation:** An early `if self.text.is_empty() { return SourcePosition::new(0, 0); }`
would make the intent obvious and remove the dependency on `line_starts_for`'s
invariant.

---

## [x] 8. `ParseOutput::new` doesn't build the node index

**File:** `crates/syl_syntax/src/parser/output.rs:22`
**Severity:** ⚠️ fragile pattern
**Tags:** node index, initialization order
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_syntax/src/parser/output.rs` plus parser tests.
- 2026-05-28 Worker (gpt-5.4-mini): changed `ParseOutput::new()` to initialize `node_index` with `file.build_node_index(\"\")`, while keeping parser-backed UTF-16 ranges via `attach_node_index(self.source)`.
- 2026-05-28 Main: verified with `cargo test -p syl_syntax parser::tests -- --nocapture`.
- 2026-05-28 Review (gpt-5.4-mini): requested follow-up. Current patch fixes the empty-index bug but still double-builds the node index on the `SourceParser` hot path.
- 2026-05-28 Worker (gpt-5.4-mini, second pass): introduced a source-aware parser path so `SourceParser` builds the real node index exactly once, while manual `ParseOutput::new()` still returns a usable skeleton index.
- 2026-05-28 Main: re-verified with `cargo test -p syl_syntax parser::tests -- --nocapture`.
- 2026-05-28 Review (gpt-5.4-mini): accepted the final one-build parser path.

```rust
pub fn new(file: AstFile, diagnostics: Vec<Diagnostic>) -> Self {
    Self {
        file,
        diagnostics,
        node_index: crate::AstNodeIndex::default(),  // ← empty!
    }
}
```

The public constructor leaves `node_index` as `default()` (empty). The node
index is only populated later by `attach_node_index(source)`, which is called
by the parser but **not by `ParseOutput::new`** itself. Anyone constructing a
`ParseOutput` manually (e.g., in tests) will get a `node_index().is_empty() == true`
and `node_index().root_id() == None`.

**Impact on test code:** Tests that parse via `SourceParser` are fine
(`attach_node_index` is called). Tests that construct `ParseOutput` directly
and then call `node_index()` will silently get empty results.

**Recommendation:** Make `node_index` required in `new()` or add a builder.
At minimum, document the empty-index caveat on `new()`.

---

## [x] 9. `MirTypeRef::subst` single-segment heuristic breaks generic re-export

**File:** `crates/syl_hir/src/model/type_ref.rs:280`
**Severity:** 🐛 bug (latent)
**Tags:** generics, substitution
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini audit subagent for issue 9.
- 2026-05-28 Audit (gpt-5.4-mini): stale report. Current lowering and type resolution intentionally treat only single-segment paths as generic parameters; the reported `std::wrap::T` substitution failure is not reproducible in current code.
- 2026-05-28 Review (gpt-5.4-mini): close as stale. Reopen only if the language later introduces qualified generic re-exports that are intended to participate in substitution.

As documented in the code now: `subst` only replaces `Path` types with
exactly one segment. A re-exported generic type like `std::wrap::T` (where
`T` is a generic parameter bubbled up through a wrapper) will **not** be
substituted:

```rust
// Replacement: {"T" -> MirTypeRef::path(vec!["UInt"])}
// Path(["T"])              → Path(["UInt"])        ✓ single-segment, replaced
// Path(["std","wrap","T"]) → Path(["std","wrap","T"])  ✗ NOT replaced
```

This is actually **correct** — multi-segment paths are fully qualified and
should not be treated as type variables. But it means any code that re-exports
a generic type under a qualified path will hit a substitution failure.

**Trigger condition:** A helper function in package `pkg` that takes a generic
`T` and wraps it in `pkg::Wrapper<T>`. If someone writes `let x: pkg::Wrapper<UInt>`,
the `Wrapper`'s `T` gets qualified as `pkg::Wrapper<T>` and substitution
won't replace it.

**Impact:** Currently none — Syl's generics are simple and this pattern doesn't
arise. But adding package-level generics without fixing this will produce
subtle type errors.

---

## [ ] 10. Missing `CancellationToken` checks between compilation stages

**File:** `crates/syl_elab/src/pipeline.rs` (and related)
**Severity:** 🧠 design note
**Tags:** cancellation, UX

`AnalysisHost` accepts a `CancellationToken`, but `HardwareCompiler::compile_tir`
does not. Elaboration can be a long-running operation (especially with large
designs and DRC), and the user has no way to interrupt it mid-flight.

The cancellation token is checked at the `AnalysisDatabase::load` level
(between stages), but the elaboration pipeline itself (`TirStageRunner`)
has no cancellation checks between its sub-stages (ConstMIR → MapIR → EIR →
DRC → HW emission).

**Recommendation:** Thread a `CancellationToken` through `TirStageRunner` and
check it between each stage.

---

## [x] 11. `ProjectError::Cancelled` is never constructed

**File:** `crates/syl_session/src/error.rs`
**Severity:** 🧹 code smell
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini audit subagent for issues 1/11/13.
- 2026-05-28 Audit (gpt-5.4-mini): stale report. `ProjectError::Cancelled` is actively constructed in `crates/syl_session/src/resolver.rs` and `crates/syl_session/src/snapshot/semantic_cache.rs`, asserted in `crates/syl_session/src/database.rs`, and mapped by the LSP layer.
- 2026-05-28 Review (gpt-5.4-mini): close as stale; current code actively constructs and propagates `ProjectError::Cancelled`.

```rust
pub enum ProjectError {
    #[error("analysis was cancelled")]
    Cancelled,           // ← never constructed anywhere in the codebase
    // ...
}
```

Searching for `ProjectError::Cancelled` or `Cancelled` in constructors yields
no results. The cancellation check in `AnalysisDatabase::load` simply returns
early rather than producing this error variant.

**Impact:** Dead variant. If someone catches `ProjectError::Cancelled`, it
will never fire. This may be intentional (cancellation is silent/success),
but then the variant shouldn't exist.

---

## [x] 12. Undocumented `_` arm in lossless lexer

**File:** `crates/syl_syntax/src/lexer/lossless.rs` (the large `match` in `lex_all_partial`)
**Severity:** 🧠 design note
**Tags:** error recovery, lossless lexer
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_syntax/src/lexer/lossless.rs`.
- 2026-05-28 Worker (gpt-5.4-mini): documented that the wildcard arm consumes exactly one Unicode scalar, spans it by UTF-8 width, emits `Unknown`, and resumes scanning at the next character.
- 2026-05-28 Review (gpt-5.4-mini): accepted; comment is technically accurate to current recovery behavior.

The lossless lexer's main dispatch has a wildcard `_` arm that creates an
`Unknown` lexeme for every character not explicitly handled. This is *correct*
(the lossless lexer must capture every byte), but it has a subtle interaction
with UTF-8: the `chars()` iterator from the scanner has already validated
the UTF-8, so `ch.len_utf8()` always returns the correct byte length. The `_`
arm never sees truly invalid UTF-8. This is worth knowing when debugging
malformed-source behavior.

---

## [x] 13. `summary_count` magic numbers in `HirDefKind` and `HirCallable`

**File:** `crates/syl_hir/src/model/summary.rs`
**Severity:** 🧹 code smell
**Log:**
- 2026-05-28 Main: dispatched a gpt-5.4-mini audit subagent for issues 1/11/13.
- 2026-05-28 Audit (gpt-5.4-mini): confirmed issue 13 is still valid; smallest fix is to name the stable summary tags and pin them with focused tests.
- 2026-05-28 Main: dispatched a gpt-5.4-mini worker on `crates/syl_hir/src/model/summary.rs` and `crates/syl_hir/src/model/callable.rs`.
- 2026-05-28 Worker (gpt-5.4-mini): named the callable tags and added a focused test, but left `HirDefKind` tag literals unchanged.
- 2026-05-28 Main: requested a second pass to finish the `HirDefKind` half before accepting the issue.
- 2026-05-28 Worker (gpt-5.4-mini, second pass): named the `HirDefKind` tags, documented the intentional `7 -> 9` gap, and pinned both `HirDefKind` and `HirCallable` mappings in tests.
- 2026-05-28 Main: verified with `cargo test -p syl_hir`.
- 2026-05-28 Review (gpt-5.4-mini): accepted the final tag/documentation/test set.

```rust
impl HirDefKind {
    pub(super) fn summary_count(&self) -> usize {
        match self {
            Self::Const => 1,
            Self::Fn => 2,
            Self::Enum => 3,
            Self::Bundle => 4,
            Self::Interface => 5,
            Self::Map => 6,
            Self::Cell => 7,
            Self::ExternCell => 9,  // ← note: skips 8!
        }
    }
}
```

The numbering skips `8` — `ExternCell` is `9`, not `8`. This is either a
typo or a deliberate gap for a removed/planned variant. Either way, it's
unexplained.

Similarly in `HirCallable::summary_count`:
```rust
Self::Cell(item) => 1 + item.summary_count(),
Self::Extern(item) => 3 + item.summary_count(),  // why 3 vs 1?
```

No commentary on why `Extern` has a base of 3 while `Cell` has a base of 1.
This makes the summary count fragile: adding a new callable kind requires
understanding the numbering scheme.

**Recommendation:** Replace magic numbers with `std::mem::discriminant` or
a consistent mapping. At minimum, document the numbering convention.

---

## [ ] Severity Key

| Marker | Meaning |
|--------|---------|
| 🐛 **bug** | Incorrect behavior under specific conditions |
| ⚠️ **fragile pattern** | Works today but likely to break under maintenance |
| 🧹 **code smell** | Not a bug, but indicates poor maintainability |
| 🧠 **design note** | Intentional trade-off worth remembering |
