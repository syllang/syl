# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0](https://github.com/syllang/syl/releases/tag/sylc-v0.1.0) - 2026-11-22

### Added

- *(syl_sim+sylc+toolchain)* productionize unified simulation backends, cost-model auto-router, CUDA cache, and VCD fault replay ([#37](https://github.com/syllang/syl/pull/37))
- *(language+sylc+toolchain)* add zero-alloc SoA CPU SIMD simulator and 2D Verilator benchmark ([#36](https://github.com/syllang/syl/pull/36))
- *(language+cli+toolchain)* add native 4-state simulator, VCD tracer, and sylc sim CLI ([#33](https://github.com/syllang/syl/pull/33))
- *(language+cli+toolchain)* add concrete word-level syl_sim IR and SCC scheduling ([#32](https://github.com/syllang/syl/pull/32))
- *(examples+tests+scripts)* add std CDC, RAM, and FIFO components with Verilator simulation ([#31](https://github.com/syllang/syl/pull/31))
- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(language+cli+toolchain)* disambiguate cross-module hardware names and emit multi-file SV ([#26](https://github.com/syllang/syl/pull/26))
- *(examples+scripts+sylc)* add std pulse/skid/arbiter components and Verilator sim harness ([#24](https://github.com/syllang/syl/pull/24))
- *(cli+ide+toolchain)* add syl.toml manifest, opaque summary loading, and sylc CLI upgrade ([#22](https://github.com/syllang/syl/pull/22))
- *(conformance+project+sylc)* expand conformance suite, README, and Verilator simulation ([#21](https://github.com/syllang/syl/pull/21))
- *(syl_elab+sylc)* lower clocked assert statements
- *(syl_sema+syl_syntax+sylc)* lower runtime error stmt to sv error
- *(frontend+backend+sylc)* lower clocked assert statements
- *(syl_elab+syl_sema+sylc)* lower runtime error statements to SystemVerilog $error
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Fixed

- *(frontend+sylc)* avoid capturing user-defined error calls
- *(syl_elab+sylc)* lower symbolic const fn conditions
- *(syl_sema+sylc)* satisfy post-rebase review gates

### Other

- *(examples+syl_emit+sylc)* add 4-shape synthesizable simulation benchmark workloads ([#34](https://github.com/syllang/syl/pull/34))
- *(docs+syl_session+sylc)* share parsed snapshots and measure workspace scaling ([#23](https://github.com/syllang/syl/pull/23))
- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(syl_sema)* derive getset accessors for semantic fact aggregates ([#18](https://github.com/syllang/syl/pull/18))
- *(syl_sema)* [**breaking**] drop empty SemanticCompiler entrypoint
- *(sylc)* split regression capability cases
- *(sylc)* align builtin fixture names
- *(sylc)* sync lowercase builtin regression fixtures
- *(sylc)* sync lowercase builtin regression fixtures
- *(sylc)* cover struct const aggregate field elaboration
- *(api+syl_elab+sylc)* align public api surface
- *(conformance+syl_elab+sylc)* align builtin fixture names

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_lsp-v0.1.0) - 2026-11-22

### Added

- *(syl_sema+ide+api)* add references, rename, workspace symbols, inlay hints, semantic tokens, and signature help ([#29](https://github.com/syllang/syl/pull/29))
- *(ide+toolchain)* add LSP manifest reload, incremental sync, and background DRC ([#28](https://github.com/syllang/syl/pull/28))
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Other

- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl-v0.1.0) - 2026-11-22

### Added

- *(language+cli+toolchain)* disambiguate cross-module hardware names and emit multi-file SV ([#26](https://github.com/syllang/syl/pull/26))
- *(cli+ide+toolchain)* add syl.toml manifest, opaque summary loading, and sylc CLI upgrade ([#22](https://github.com/syllang/syl/pull/22))
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_query-v0.1.0) - 2026-11-22

### Added

- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(syl_sema+ide+api)* add references, rename, workspace symbols, inlay hints, semantic tokens, and signature help ([#29](https://github.com/syllang/syl/pull/29))
- *(ide+toolchain)* add LSP manifest reload, incremental sync, and background DRC ([#28](https://github.com/syllang/syl/pull/28))
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Other

- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(syl_syntax)* split Parser methods by concern ([#14](https://github.com/syllang/syl/pull/14))
- *(frontend+backend+ide)* expand pipeline public type rustdoc

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_session-v0.1.0) - 2026-11-22

### Added

- *(ide+toolchain)* add LSP manifest reload, incremental sync, and background DRC ([#28](https://github.com/syllang/syl/pull/28))
- *(language+cli+toolchain)* disambiguate cross-module hardware names and emit multi-file SV ([#26](https://github.com/syllang/syl/pull/26))
- *(frontend+ide+toolchain)* complete fn/map type checks, cycle diagnostics, and duplicate module errors ([#25](https://github.com/syllang/syl/pull/25))
- *(cli+ide+toolchain)* add syl.toml manifest, opaque summary loading, and sylc CLI upgrade ([#22](https://github.com/syllang/syl/pull/22))
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Other

- *(docs+syl_session+sylc)* share parsed snapshots and measure workspace scaling ([#23](https://github.com/syllang/syl/pull/23))
- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(frontend+backend+ide)* expand pipeline public type rustdoc
- *(syl_sema)* [**breaking**] drop empty SemanticCompiler entrypoint

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_elab-v0.1.0) - 2026-11-22

### Added

- *(language+cli+toolchain)* add native 4-state simulator, VCD tracer, and sylc sim CLI ([#33](https://github.com/syllang/syl/pull/33))
- *(language+cli+toolchain)* add concrete word-level syl_sim IR and SCC scheduling ([#32](https://github.com/syllang/syl/pull/32))
- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(language+cli+toolchain)* disambiguate cross-module hardware names and emit multi-file SV ([#26](https://github.com/syllang/syl/pull/26))
- *(frontend+ide+toolchain)* complete fn/map type checks, cycle diagnostics, and duplicate module errors ([#25](https://github.com/syllang/syl/pull/25))
- *(syl_elab+sylc)* lower clocked assert statements
- *(syl_elab+syl_sema)* close deterministic elaboration numbering
- *(syl_elab+syl_sema)* close mutable elaboration local loop
- *(frontend+backend+sylc)* lower clocked assert statements
- *(syl_elab+syl_sema+sylc)* lower runtime error statements to SystemVerilog $error
- *(frontend)* implement structured const software structs
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Fixed

- *(syl_elab)* drop stale assertion helper imports
- *(syl_elab)* drop stale TirType import after replay
- *(syl_elab)* finalize rfc-007 rebase repair
- *(syl_elab)* unblock rfc-007 clippy rebase
- *(frontend+sylc)* avoid capturing user-defined error calls
- *(syl_elab)* restrict numbering summary concretization
- *(syl_elab+sylc)* lower symbolic const fn conditions
- *(syl_elab)* preserve symbolic software struct merges
- *(syl_elab)* materialize struct locals for const eval
- *(syl_syntax+syl_sema+syl_elab)* close typed mutable struct locals and parser shadowing
- *(syl_elab)* preserve mutable local value semantics through control flow
- *(syl_elab+syl_sema)* close mutable local state propagation and field assign
- *(syl_syntax+syl_sema+syl_elab)* restore mutable local elaboration candidate
- *(syl_elab)* split statement assert helpers
- *(syl_elab+syl_hir+syl_sema)* resolve struct const defs by visible owner
- *(syl_elab+syl_sema+examples)* complete lowercase builtin type migration

### Other

- *(syl_elab+syl_sema)* restore partial TIR recovery tests and clear rustdoc warnings ([#20](https://github.com/syllang/syl/pull/20))
- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(syl_hir)* [**breaking**] encapsulate HirDesign fields behind getset accessors ([#16](https://github.com/syllang/syl/pull/16))
- *(frontend+backend+ide)* expand pipeline public type rustdoc
- *(syl_sema)* [**breaking**] drop empty SemanticCompiler entrypoint
- *(syl_elab)* split statement builtin lowering helpers
- *(syl_sema+syl_elab)* format stacked rebase follow-up
- *(syl_elab)* split next lowering helpers
- *(syl_elab)* drop unused numbering request shims
- *(syl_elab)* tighten numbering helper boundaries
- *(syl_elab)* format numbering rebase follow-up
- *(syl_elab)* split numbering request structs
- *(syl_sema+syl_elab)* split oversized body helpers
- *(syl_elab)* simplify local elaboration helpers
- *(syl_elab+syl_sema+syl_syntax)* apply rustfmt baseline fixes
- *(api+syl_elab+sylc)* align public api surface
- *(conformance+syl_elab+sylc)* align builtin fixture names

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_sema-v0.1.0) - 2026-11-22

### Added

- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(syl_sema+ide+api)* add references, rename, workspace symbols, inlay hints, semantic tokens, and signature help ([#29](https://github.com/syllang/syl/pull/29))
- *(frontend+ide+toolchain)* complete fn/map type checks, cycle diagnostics, and duplicate module errors ([#25](https://github.com/syllang/syl/pull/25))
- *(syl_sema+syl_syntax+sylc)* lower runtime error stmt to sv error
- *(syl_sema)* tighten returned endpoint field access
- *(syl_elab+syl_sema)* close deterministic elaboration numbering
- *(syl_elab+syl_sema)* close mutable elaboration local loop
- *(frontend+backend+sylc)* lower clocked assert statements
- *(syl_elab+syl_sema+sylc)* lower runtime error statements to SystemVerilog $error
- *(syl_sema)* tighten returned endpoint field access
- *(frontend)* implement structured const software structs
- *(syl_sema+syl_syntax)* lowercase software builtin type names
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Fixed

- *(syl_sema)* coerce constant nats in hardware branches and map returns ([#27](https://github.com/syllang/syl/pull/27))
- *(frontend+sylc)* avoid capturing user-defined error calls
- *(syl_sema)* thread assert stmt context through control blocks
- *(syl_sema)* close struct const fn call path
- *(syl_sema)* rewrite fn struct field assignments
- *(syl_syntax+syl_sema+syl_elab)* close typed mutable struct locals and parser shadowing
- *(syl_elab+syl_sema)* close mutable local state propagation and field assign
- *(syl_syntax+syl_sema+syl_elab)* restore mutable local elaboration candidate
- *(syl_sema+sylc)* satisfy post-rebase review gates
- *(syl_sema)* gate test-only const evaluator helper
- *(syl_elab+syl_hir+syl_sema)* resolve struct const defs by visible owner
- *(syl_elab+syl_sema+examples)* complete lowercase builtin type migration

### Other

- *(syl_elab+syl_sema)* restore partial TIR recovery tests and clear rustdoc warnings ([#20](https://github.com/syllang/syl/pull/20))
- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(syl_sema)* derive getset accessors for semantic fact aggregates ([#18](https://github.com/syllang/syl/pull/18))
- *(syl_sema)* encapsulate TirDesign behind getset and builder ([#17](https://github.com/syllang/syl/pull/17))
- *(syl_hir)* [**breaking**] encapsulate HirDesign fields behind getset accessors ([#16](https://github.com/syllang/syl/pull/16))
- *(syl_sema)* split HirResolver insert path into its own module ([#15](https://github.com/syllang/syl/pull/15))
- *(frontend+backend+ide)* expand pipeline public type rustdoc
- *(syl_sema)* [**breaking**] drop empty SemanticCompiler entrypoint
- *(syl_sema+syl_elab)* format stacked rebase follow-up
- *(syl_sema)* split semantic facts coverage
- *(syl_sema)* split const_mir lower tests
- *(syl_sema+syl_elab)* split oversized body helpers
- *(syl_sema)* apply rustfmt to elaboration body check
- *(syl_sema)* narrow elaboration check helper params
- *(syl_elab+syl_sema+syl_syntax)* apply rustfmt baseline fixes
- *(syl_sema)* trim capability checker line budget
- *(syl_sema)* split capability field schema helpers
- *(syl_sema)* apply rustfmt baseline fixes
- *(syl_sema)* split const mir and semantic facts
- *(syl_hir+syl_sema+syl_syntax)* apply rustfmt baseline fixes
- *(syl_sema)* apply rustfmt baseline fixes

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_hir-v0.1.0) - 2026-11-22

### Added

- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(frontend)* implement structured const software structs
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Fixed

- *(syl_elab+syl_hir+syl_sema)* resolve struct const defs by visible owner

### Other

- *(syl_hir)* [**breaking**] encapsulate HirDesign fields behind getset accessors ([#16](https://github.com/syllang/syl/pull/16))
- *(syl_hir+syl_sema+syl_syntax)* apply rustfmt baseline fixes

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_syntax-v0.1.0) - 2026-11-22

### Added

- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(syl_sema+syl_syntax+sylc)* lower runtime error stmt to sv error
- *(frontend)* implement structured const software structs
- *(syl_sema+syl_syntax)* lowercase software builtin type names
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Fixed

- *(frontend+sylc)* avoid capturing user-defined error calls
- *(syl_syntax+syl_sema+syl_elab)* close typed mutable struct locals and parser shadowing
- *(syl_syntax+syl_sema+syl_elab)* restore mutable local elaboration candidate

### Other

- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(syl_syntax)* split Parser methods by concern ([#14](https://github.com/syllang/syl/pull/14))
- *(syl_sema)* [**breaking**] drop empty SemanticCompiler entrypoint
- *(syl_elab+syl_sema+syl_syntax)* apply rustfmt baseline fixes
- *(syl_hir+syl_sema+syl_syntax)* apply rustfmt baseline fixes

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_emit-v0.1.0) - 2026-11-22

### Added

- *(language+cli+toolchain)* add native 4-state simulator, VCD tracer, and sylc sim CLI ([#33](https://github.com/syllang/syl/pull/33))
- *(language+cli+toolchain)* add concrete word-level syl_sim IR and SCC scheduling ([#32](https://github.com/syllang/syl/pull/32))
- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(language+cli+toolchain)* disambiguate cross-module hardware names and emit multi-file SV ([#26](https://github.com/syllang/syl/pull/26))
- *(frontend+backend+sylc)* lower clocked assert statements
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Fixed

- *(syl_emit)* normalize bool param literals

### Other

- *(examples+syl_emit+sylc)* add 4-shape synthesizable simulation benchmark workloads ([#34](https://github.com/syllang/syl/pull/34))
- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
- *(frontend+backend+ide)* expand pipeline public type rustdoc

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_hw-v0.1.0) - 2026-11-22

### Added

- *(language+cli+toolchain)* add concrete word-level syl_sim IR and SCC scheduling ([#32](https://github.com/syllang/syl/pull/32))
- *(language+sylc+api)* add CDC static checks, reset polarity qualifiers, and RAM primitives ([#30](https://github.com/syllang/syl/pull/30))
- *(language+cli+toolchain)* disambiguate cross-module hardware names and emit multi-file SV ([#26](https://github.com/syllang/syl/pull/26))
- *(frontend+backend+sylc)* lower clocked assert statements
- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Other

- *(frontend+backend+ide)* expand pipeline public type rustdoc

## [0.1.0](https://github.com/syllang/syl/releases/tag/syl_span-v0.1.0) - 2026-11-22

### Added

- *(.)* add initial Syl compiler baseline ([#1](https://github.com/syllang/syl/pull/1))

### Other

- *(language+sylc+toolchain)* move analysis ownership to Salsa queries ([#19](https://github.com/syllang/syl/pull/19))
# Changelog

All notable changes to Syl will be documented in this file.
