use super::{SvExpr, SvPrinter};
use crate::sv_ir::SvMessage;
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
    pub(super) fn clocked_messages(
        &mut self,
        clock: &SvExpr,
        logs: &[SvMessage],
        assertions: &[SvMessage],
        indent: usize,
    ) {
        let pad = " ".repeat(indent);
        writeln!(self.out, "{pad}`ifndef SYNTHESIS").unwrap();
        let id = self.failure_groups;
        let message = format!("syl$failure{id}");
        let event = format!("syl$report{id}");
        if !assertions.is_empty() {
            self.failure_groups += 1;
            // The valid bit suppresses sensitivity-list initialization at time
            // zero. The low bit toggles on every failure, including repeats.
            writeln!(self.out,"{pad}string {message};\n{pad}bit [1:0] {event};\n{pad}always @({event}) if ({event}[1]) $error(\"%s\", {message});").unwrap();
        }
        write!(self.out, "{pad}always_ff @(posedge ").unwrap();
        self.expr(clock);
        self.out.push_str(") begin\n");
        for log in logs {
            self.doc(indent + 2, log.source.as_deref());
            write!(self.out, "{pad}  if (").unwrap();
            self.expr(&log.trigger);
            self.out.push_str(") ");
            self.message("$display", &log.pieces, &pad);
        }
        for (i, assertion) in assertions.iter().enumerate() {
            self.doc(indent + 2, assertion.source.as_deref());
            let branch = if i == 0 { "if" } else { "else if" };
            write!(self.out, "{pad}  {branch} (").unwrap();
            self.expr(&assertion.trigger);
            self.out.push_str(") begin\n");
            // Capture old values here; notify the reporter in the NBA region
            // after every clocked observer has printed its sampled logs.
            write!(self.out, "{pad}    ").unwrap();
            self.message(
                &format!("{message} = $sformatf"),
                &assertion.pieces,
                &format!("{pad}  "),
            );
            writeln!(
                self.out,
                "{pad}    {event} <= {{1'b1, ~{event}[0]}};\n{pad}  end"
            )
            .unwrap();
        }
        writeln!(self.out, "{pad}end\n{pad}`endif").unwrap();
    }

    fn message(&mut self, task: &str, pieces: &[FormatPiece<SvExpr>], pad: &str) {
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

    fn hex(&mut self, value: &str, field: &str, pad: &str, upper: bool) {
        self.uses_hex = true;
        writeln!(
            self.out,
            "{pad}{field} = syl$hex($sformatf(\"%b\", {value}), 1'b{});",
            u8::from(upper)
        )
        .unwrap();
    }

    fn field(&mut self, style: &FormatStyle, i: usize, pad: &str) {
        let v = format!("syl$v{i}");
        let f = format!("syl$f{i}");
        match style {
            FormatStyle::Hex { .. } | FormatStyle::Binary { .. } => {
                let (prefix, digits) = match style {
                    FormatStyle::Hex {
                        upper,
                        prefix,
                        digits,
                    } => {
                        self.hex(&v, &f, pad, *upper);
                        (
                            if !prefix {
                                ""
                            } else if *upper {
                                "0X"
                            } else {
                                "0x"
                            },
                            digits,
                        )
                    }
                    FormatStyle::Binary { prefix, digits } => {
                        writeln!(self.out, "{pad}{f} = $sformatf(\"%b\", {v});").unwrap();
                        (if *prefix { "0b" } else { "" }, digits)
                    }
                    _ => unreachable!(),
                };
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
                self.hex(&v, &f, pad, false);
                writeln!(self.out,"{pad}if ($bits({v}) == 1) {f} = $sformatf(\"1'b%b\", {v}); else {f} = $sformatf(\"%0d'h%s\", $bits({v}), {f});").unwrap();
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
