use syl_elab::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_syntax::SourceParser;

struct BundleHarness {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl BundleHarness {
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
fn map_bundle_field_uses_owner_type_scope() {
    let verilog = BundleHarness::new()
        .compile_sources(&[
            r#"
package lib;

bundle Pair {
    hi: Bit,
    lo: Bit,
}

map high(pair: Pair) -> Bit =
    pair.hi

module LibTop(x: in Bit, y: out Bit) {
    signal pair: Pair := Pair {
        hi: x,
        lo: 0,
    }
    y := high(pair)
}
"#,
            r#"
package app;

bundle Pair {
    only: UInt<4>,
}
"#,
        ])
        .expect("EIR map/bundle lowering must resolve bundle fields through the map owner");

    assert!(verilog.contains("assign pair = {x, 0};"));
    assert!(verilog.contains("assign y = pair["));
}
