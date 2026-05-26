mod support;

use support::MiddleCompiler;
use syl_emit::SystemVerilogBackend;
use syl_syntax::SourceParser;

struct GenerateLabelHarness {
    middle: MiddleCompiler,
    backend: SystemVerilogBackend,
}

impl GenerateLabelHarness {
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
fn uniquifies_generate_labels_for_repeated_cell_expansions() {
    let verilog = GenerateLabelHarness::new()
        .compile(
            r#"
cell Maybe<E: Bool>() -> y: Bit {
    if E {
        y := 1
    } else {
        y := 0
    }
}

cell Top<A: Bool, B: Bool>(a: out Bit, b: out Bit) {
    let u = inplace Maybe<A>()
    let v = inplace Maybe<B>()
    a := u
    b := v
}
"#,
        )
        .expect("repeated inplace symbolic generators must produce legal SV labels");

    assert!(verilog.contains("begin : gen_if_u_"));
    assert!(verilog.contains("begin : gen_if_v_"));

    let labels = verilog
        .lines()
        .filter(|line| line.contains("if (") && line.contains("begin : gen_if_"))
        .filter_map(|line| line.split_once("begin : ").map(|(_, label)| label.trim()))
        .collect::<Vec<_>>();

    assert_eq!(labels.len(), 3);
    assert_ne!(labels[0], labels[1]);
}
