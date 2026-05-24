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
package lib;

const ENABLE: Bool = true

module LibTop(y: out Bit) {
    if ENABLE {
        y := 1
    } else {
        y := 0
    }
}
"#,
            r#"
package app;

const ENABLE: Nat = 0
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
package lib;

fn choose(x: Nat) -> Bool {
    return x == 1
}

module LibTop(y: out Bit) {
    if choose(1) {
        y := 1
    } else {
        y := 0
    }
}
"#,
            r#"
package app;

fn choose(x: Bool) -> Bool {
    return false
}
"#,
        ])
        .expect("same-leaf fn in another package must not poison owner-scoped const calls");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = 0;"));
}

#[test]
fn enum_variants_are_scoped_by_enum_definition() {
    let verilog = ConstResolutionHarness::new()
        .compile_sources(&[r#"
package lib;

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

module Top(x: in Left, y: out Bit) {
    y := is_left_same(x)
}
"#])
        .expect("same variant names in different enums must not collide globally");

    assert!(verilog.contains("assign y = ((x == 0) ? 1 : 0);"));
}
