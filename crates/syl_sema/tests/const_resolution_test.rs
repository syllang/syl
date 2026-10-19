mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_syntax::SourceParser;

struct ConstResolutionHarness {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl ConstResolutionHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
            backend: SystemVerilogBackend::new(),
        }
    }

    fn compile_sources(&self, sources: &[&str]) -> Result<String, String> {
        let mut files = Vec::new();
        for source in sources {
            files.push(SourceParser::new(source).parse_file().map_err(|errs| {
                errs.iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n")
            })?);
        }
        let hwir = self
            .middle
            .compile_files(&files)
            .map_err(|err| err.to_string())?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }
}

#[test]
fn elaboration_const_uses_owner_scope_not_global_leaf_name() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[
            r#"
const ENABLE: bool = true

cell LibTop(y: out Bit) {
    if ENABLE {
        y := 1
    } else {
        y := 0
    }
}
"#,
            r#"
const ENABLE: nat = 0
"#,
        ])
        .expect("same-leaf const in another package must not poison owner-scoped elaboration");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
}

#[test]
fn elaboration_fn_call_uses_owner_scope_not_global_leaf_name() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[
            r#"
fn choose(x: nat) -> bool {
    return x == 1
}

cell LibTop(y: out Bit) {
    if choose(1) {
        y := 1
    } else {
        y := 0
    }
}
"#,
            r#"
fn choose(x: bool) -> bool {
    return false
}
"#,
        ])
        .expect("same-leaf fn in another package must not poison owner-scoped const calls");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
}

#[test]
fn elaboration_const_fn_call_accepts_struct_aggregate_args() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
struct Config {
    enabled: bool,
}

fn choose(cfg: Config) -> bool {
    return cfg.enabled
}

cell Top(y: out Bit) {
    if choose(Config { enabled: true }) {
        y := 1
    } else {
        compile_error("unreachable")
    }
}
"#])
        .expect("struct-valued const-fn args must elaborate through const call evaluation");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("unreachable"));
}

#[test]
fn elaboration_const_fn_struct_return_supports_const_field_access() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
struct Config {
    enabled: bool,
}

fn enable(cfg: Config) -> Config {
    return Config { enabled: !cfg.enabled }
}

const CFG: Config = enable(Config { enabled: false })

cell Top(y: out Bit) {
    if CFG.enabled {
        y := 1
    } else {
        compile_error("unreachable")
    }
}
"#])
        .expect("struct-valued const-fn returns must remain usable through const field access");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("unreachable"));
}

#[test]
fn rejects_uppercase_nat_and_bool_in_const_phase_items() {
    let err = ConstResolutionHarness::new()
        .compile_sources(&[r#"
const ENABLE: Bool = true

fn choose(x: Nat) -> Bool {
    return x == 1
}

cell Top(y: out Bit) {
    if choose(1) {
        y := 1
    } else {
        y := 0
    }
}
"#])
        .expect_err("uppercase const-phase builtin names must be rejected");

    assert!(
        err.contains("unknown type Bool") || err.contains("unknown type Nat"),
        "{err}"
    );
}

#[test]
fn enum_variants_are_scoped_by_enum_definition() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum Left {
    Same,
    Other,
}

enum Right {
    Same,
}

map is_left_same(x: Left) -> Bit =
    match x {
        .Left.Same => 1,
        default => 0,
    }

cell Top(x: in Left, y: out Bit) {
    y := is_left_same(x)
}
"#])
        .expect("same variant names in different enums must not collide globally");

    assert!(verilog.contains("assign y = ((x == 0) ? 1 : 0);"));
}

#[test]
fn shorthand_enum_match_patterns_lower_against_scrutinee_type() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum State {
    Idle,
    Busy,
}

map is_idle(x: State) -> Bit =
    match x {
        .Idle => 1,
        default => 0,
    }

cell Top(x: in State, y: out Bit) {
    y := is_idle(x)
}
"#])
        .expect("match shorthand should resolve against the scrutinee enum type");

    assert!(verilog.contains("assign y = ((x == 0) ? 1 : 0);"));
}

#[test]
fn shorthand_enum_match_patterns_work_in_cell_bodies() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum State {
    Idle,
    Busy,
}

cell Top(x: in State, y: out Bit) {
    y := match x {
        .Idle => 1,
        default => 0,
    }
}
"#])
        .expect("cell-local match shorthand should resolve against the scrutinee enum type");

    assert!(verilog.contains("assign y = ((x == 0) ? 1 : 0);"));
}

#[test]
fn enum_variant_in_expression_context() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum State {
    IDLE,
    BUSY,
    DONE,
}

map is_idle(x: State) -> Bit =
    x eq State.IDLE

cell Top(x: in State, y: out Bit) {
    y := is_idle(x)
}
"#])
        .expect("qualified enum variant name must resolve in expression context");

    assert!(verilog.contains("assign y = (x == 0);"));
}

#[test]
fn bare_enum_variant_in_expression_context_is_rejected() {
    let err = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum State {
    IDLE,
}

map is_idle(x: State) -> Bit =
    x eq IDLE

cell Top(x: in State, y: out Bit) {
    y := is_idle(x)
}
"#])
        .expect_err("bare enum variant names should stay unresolved in expressions");

    assert!(err.contains("unresolved name IDLE"), "{err}");
}

#[test]
fn enum_width_uses_max_discriminant_value() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum State {
    Idle = 1,
    Busy = 4,
    Done = 7,
}

map is_done(x: State) -> Bit =
    x eq State.Done

cell Top(x: in State, y: out Bit) {
    y := is_done(x)
}
"#])
        .expect("enum width should follow the highest discriminant value");

    assert!(verilog.contains("input [2:0] x"));
    assert!(verilog.contains("assign y = (x == 7);"));
}

#[test]
fn flags_layout_uses_one_hot_values() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
@layout(flags)
enum Access {
    Read,
    Write,
    Exec,
    Admin,
}

map has_admin(x: Access) -> Bit =
    x eq Access.Admin

cell Top(x: in Access, y: out Bit) {
    y := has_admin(x)
}
"#])
        .expect("flags layout should emit one-hot discriminants and width");

    assert!(verilog.contains("input [3:0] x"));
    assert!(verilog.contains("assign y = (x == 8);"));
}

#[test]
fn flags_layout_allows_zero_discriminant() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
@layout(flags)
enum Access {
    None = 0,
    Read,
}

map is_none(x: Access) -> Bit =
    x eq Access.None

cell Top(x: in Access, y: out Bit) {
    y := is_none(x)
}
"#])
        .expect("flags layout should allow the empty-set discriminant");

    assert!(verilog.contains("assign y = (x == 0);"));
}

#[test]
fn onehot_layout_rejects_zero_discriminant() {
    let err = ConstResolutionHarness::new()
        .compile_sources(&[r#"
@layout(onehot)
enum State {
    Idle = 0,
}
"#])
        .expect_err("onehot layout must reject zero discriminants");

    assert!(err.contains("is not one-hot"), "{err}");
}

#[test]
fn rejects_duplicate_enum_discriminants() {
    let err = ConstResolutionHarness::new()
        .compile_sources(&[r#"
enum State {
    Idle = 1,
    Busy = 1,
}
"#])
        .expect_err("duplicate discriminants must be rejected");

    assert!(err.contains("duplicate enum discriminant 1"), "{err}");
}

#[test]
fn rejects_invalid_flags_discriminants() {
    let err = ConstResolutionHarness::new()
        .compile_sources(&[r#"
@layout(flags)
enum Access {
    Read = 3,
}
"#])
        .expect_err("flags discriminants must stay one-hot");

    assert!(err.contains("is not one-hot"), "{err}");
}
