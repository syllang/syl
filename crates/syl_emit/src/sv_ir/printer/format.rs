use super::{SvExpr, SvPrinter};
use std::fmt::Write;
use syl_hw::{FormatPiece, FormatStyle};

fn quoted(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

impl SvPrinter<'_> {
    pub(super) fn message(&mut self, log: bool, pieces: &[FormatPiece<SvExpr>], pad: &str) {
        let task = if log { "$display" } else { "$error" };
        let mut format = String::new();
        let mut fields = Vec::new();
        for piece in pieces {
            match piece {
                FormatPiece::Literal(text) => format.push_str(&text.replace('%', "%%")),
                FormatPiece::Value { expr, width, style } => {
                    format.push_str("%s");
                    fields.push((expr, width, style));
                }
            }
        }
        if fields.is_empty() {
            writeln!(self.out, "{task}({});", quoted(&format)).unwrap();
            return;
        }
        let body = format!("{pad}    ");
        self.out.push_str("begin\n");
        for (i, (expr, width, _)) in fields.iter().enumerate() {
            write!(self.out, "{body}string syl$f{i};\n{body}logic [").unwrap();
            if let Some(w) = width {
                write!(self.out, "({w})").unwrap();
            } else {
                self.out.push_str("$bits(");
                self.expr(expr);
                self.out.push(')');
            }
            writeln!(self.out, "-1:0] syl$v{i};").unwrap();
        }
        for (i, (expr, _, style)) in fields.iter().enumerate() {
            write!(self.out, "{body}syl$v{i} = ").unwrap();
            self.expr(expr);
            self.out.push_str(";\n");
            self.field(style, i, &body);
        }
        write!(self.out, "{body}{task}({}", quoted(&format)).unwrap();
        for i in 0..fields.len() {
            write!(self.out, ", syl$f{i}").unwrap();
        }
        writeln!(self.out, ");\n{pad}  end").unwrap();
    }

    fn field(&mut self, style: &FormatStyle, i: usize, pad: &str) {
        let v = format!("syl$v{i}");
        let f = format!("syl$f{i}");
        match style {
            FormatStyle::Hex { .. } | FormatStyle::Binary { .. } => {
                let (code, prefix, digits, upper) = match style {
                    FormatStyle::Hex {
                        upper,
                        prefix,
                        digits,
                    } => (
                        "h",
                        if !prefix {
                            ""
                        } else if *upper {
                            "0X"
                        } else {
                            "0x"
                        },
                        digits,
                        *upper,
                    ),
                    FormatStyle::Binary { prefix, digits } => {
                        ("b", if *prefix { "0b" } else { "" }, digits, false)
                    }
                    _ => unreachable!(),
                };
                writeln!(self.out, "{pad}{f} = $sformatf(\"%{code}\", {v});").unwrap();
                if upper {
                    writeln!(self.out,"{pad}for (int syl$j=0; syl$j<{f}.len(); syl$j++) if ({f}[syl$j] >= 8'd97 && {f}[syl$j] <= 8'd102) {f}[syl$j] = {f}[syl$j] - 8'd32;").unwrap();
                }
                if *digits > 0 {
                    writeln!(
                        self.out,
                        "{pad}while ({f}.len() < {digits}) {f} = {{\"0\", {f}}};"
                    )
                    .unwrap();
                }
                if !prefix.is_empty() {
                    writeln!(self.out, "{pad}{f} = {{\"{prefix}\", {f}}};").unwrap();
                }
            }
            FormatStyle::Unsigned | FormatStyle::Signed => {
                let cast = if *style == FormatStyle::Signed {
                    "$signed"
                } else {
                    "$unsigned"
                };
                writeln!(self.out,"{pad}if ((^{v}) === 1'bx) {f} = $sformatf(\"%b\", {v}); else {f} = $sformatf(\"%0d\", {cast}({v}));").unwrap();
            }
            FormatStyle::Bool => {
                writeln!(self.out,"{pad}case ({v})\n{pad}  1'b0: {f} = \"false\";\n{pad}  1'b1: {f} = \"true\";\n{pad}  default: {f} = $sformatf(\"%b\", {v});\n{pad}endcase").unwrap();
            }
            FormatStyle::Debug => {
                writeln!(self.out,"{pad}if ($bits({v}) == 1) {f} = $sformatf(\"1'b%b\", {v}); else {f} = $sformatf(\"%0d'h%h\", $bits({v}), {v});").unwrap();
            }
            FormatStyle::Enum {
                name,
                variants,
                debug,
            } => {
                writeln!(self.out, "{pad}case ({v})").unwrap();
                for (raw, variant) in variants {
                    let label = format!("{name}.{variant}");
                    let value = if *debug {
                        format!("$sformatf(\"{label}(%0d'd%0d)\", $bits({v}), {v})")
                    } else {
                        quoted(&label)
                    };
                    writeln!(self.out, "{pad}  {raw}: {f} = {value};").unwrap();
                }
                writeln!(self.out,"{pad}  default: if ((^{v}) === 1'bx) {f} = $sformatf(\"{name}(%0d'b%b)\", $bits({v}), {v}); else {f} = $sformatf(\"{name}(%0d)\", {v});\n{pad}endcase").unwrap();
            }
        }
    }
}
