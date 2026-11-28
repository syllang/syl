use super::*;
use num_bigint::{BigInt, BigUint};
use syl_hw::{FormatPiece, FormatStyle};

impl Simulator<'_> {
    pub(super) fn sample_logs(&mut self, rising: &BTreeSet<DomainId>) -> Result<()> {
        for log in &self.design.logs {
            let site = (&log.source).into();
            if rising.contains(&log.clock)
                && self.eval(&log.trigger, 1, site)?.truth() == Logic::One
            {
                let message = self.format_message(&log.message, site)?;
                let cycle = self.stimulus_step.unwrap_or_else(|| self.cycle() + 1);
                let line = format!("[cycle {cycle}] {}: {message}", log.source.path);
                if let Some(sink) = &mut self.log_sink {
                    sink(&line)
                } else {
                    self.log_lines.push(line)
                }
            }
        }
        Ok(())
    }

    pub(super) fn format_message(
        &mut self,
        parts: &[FormatPiece<SimExpr, u32>],
        site: Site,
    ) -> Result<String> {
        let mut out = String::new();
        for part in parts {
            match part {
                FormatPiece::Literal(text) => out.push_str(text),
                FormatPiece::Value { expr, width, style } => {
                    let v = self.eval(expr, width.unwrap_or(0), site)?;
                    let v = if let Some(w) = width {
                        v.resized(*w)
                    } else {
                        v
                    };
                    out.push_str(&format_value(&v, style));
                }
            }
        }
        Ok(out)
    }
}

fn decimal(v: &Value, signed: bool) -> String {
    if !v.is_known() {
        return v.to_binary();
    }
    let unsigned = v.unsigned_integer();
    if signed && v.bit(v.width() - 1) == Logic::One {
        (BigInt::from(unsigned) - BigInt::from(BigUint::from(1u8) << v.width())).to_string()
    } else {
        unsigned.to_string()
    }
}

fn format_value(v: &Value, style: &FormatStyle) -> String {
    let w = v.width();
    match style {
        FormatStyle::Unsigned => decimal(v, false),
        FormatStyle::Signed => decimal(v, true),
        FormatStyle::Hex {
            upper,
            prefix,
            digits,
        } => {
            let mut hex = v.to_hex();
            if *upper {
                hex = hex
                    .chars()
                    .map(|c| {
                        if c.is_ascii_hexdigit() {
                            c.to_ascii_uppercase()
                        } else {
                            c
                        }
                    })
                    .collect();
            }
            let prefix = match (prefix, upper) {
                (true, true) => "0X",
                (true, false) => "0x",
                _ => "",
            };
            format!("{prefix}{hex:0>width$}", width = usize::from(*digits))
        }
        FormatStyle::Binary { prefix, digits } => format!(
            "{}{:0>width$}",
            if *prefix { "0b" } else { "" },
            v.to_binary(),
            width = usize::from(*digits)
        ),
        FormatStyle::Bool => match v.bit(0) {
            Logic::One => "true".into(),
            Logic::Zero => "false".into(),
            _ => v.to_binary(),
        },
        FormatStyle::Debug if w == 1 => format!("1'b{}", v.to_binary()),
        FormatStyle::Debug => format!("{w}'h{}", v.to_hex()),
        FormatStyle::Enum {
            name,
            variants,
            debug,
        } => {
            if let Some(raw) = v.as_u64()
                && let Some((_, variant)) = variants.iter().find(|(n, _)| *n == raw)
            {
                if *debug {
                    format!("{name}.{variant}({w}'d{raw})")
                } else {
                    format!("{name}.{variant}")
                }
            } else if v.is_known() {
                format!("{name}({})", decimal(v, false))
            } else {
                format!("{name}({w}'b{})", v.to_binary())
            }
        }
    }
}
