mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_syntax::SourceParser;

struct AliasHarness {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl AliasHarness {
    fn new() -> Self {
        Self {
            middle: MiddleCompiler::new(),
            backend: SystemVerilogBackend::new(),
        }
    }

    fn compile(&self, source: &str) -> Result<String, String> {
        let file = SourceParser::new(source).parse_file().map_err(|errs| {
            errs.iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join("\n")
        })?;
        let hwir = self
            .middle
            .compile_files(&[file])
            .map_err(|err| err.to_string())?;
        self.backend.emit(&hwir).map_err(|err| err.to_string())
    }
}

#[test]
fn alias_map_call_binds_expression_not_instance() {
    let verilog = AliasHarness::new()
        .compile(
            r#"
map choose(x: Bit) -> Bit =
    x

cell Top(x: in Bit, y: out Bit) {
    let selected = choose(x)
    y := selected
}
"#,
        )
        .expect("let of a map call must lower as an expression let");

    assert!(verilog.contains("assign y = x;"));
    assert!(!verilog.contains("choose_inst"));
    assert!(!verilog.contains("module choose"));
}

#[test]
fn user_map_named_zero_shadows_builtin_zero() {
    let verilog = AliasHarness::new()
        .compile(
            r#"
map zero() -> Bit =
    1

cell Top(y: out Bit) {
    y := zero()
}
"#,
        )
        .expect("normal name resolution must run before builtin zero fallback");

    assert!(verilog.contains("assign y = 1;"));
    assert!(!verilog.contains("assign y = '0;"));
}
