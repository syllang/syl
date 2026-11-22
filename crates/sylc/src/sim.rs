use crate::{cli::SimArgs, cli_error};
use serde::Deserialize;
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::BufWriter,
    path::Path,
};
use syl_sim::*;
use syl_span::{Diagnostic, Span};
type Result<T> = std::result::Result<T, Box<Diagnostic>>;

fn error(message: impl ToString) -> Box<Diagnostic> {
    cli_error("E_SIM", message.to_string())
}
fn located(span: Span, message: impl ToString) -> Box<Diagnostic> {
    Box::new(
        Diagnostic::new(span, message.to_string())
            .with_code("E_SIM")
            .with_source("sylc"),
    )
}
pub(super) fn design_error(e: SimError) -> Box<Diagnostic> {
    match &e {
        SimError::CombinationalCycle { span, .. } => located(*span, e),
        _ => error(e),
    }
}
fn runtime(e: RuntimeError) -> Box<Diagnostic> {
    match &e {
        RuntimeError::Assertion(f) => located(f.span, e),
        RuntimeError::UniqueOverlap(v) => located(v.span, e),
        _ => error(e),
    }
}
pub(super) fn parameters(args: &[String]) -> Result<BTreeMap<String, ParamValue>> {
    let mut out = BTreeMap::new();
    for p in args {
        let (k, v) = p
            .split_once('=')
            .ok_or_else(|| error(format!("expected --param K=V, got {p}")))?;
        let value = match v {
            "true" => ParamValue::Bool(true),
            "false" => ParamValue::Bool(false),
            _ => ParamValue::Nat(
                v.parse()
                    .map_err(|_| error(format!("invalid parameter {p}")))?,
            ),
        };
        if out.insert(k.to_owned(), value).is_some() {
            return Err(error(format!("duplicate parameter {k}")));
        }
    }
    Ok(out)
}
#[derive(Deserialize)]
#[serde(untagged)]
enum Literal {
    UInt(u64),
    Bool(bool),
    Bits(String),
}
impl Literal {
    fn value(&self, width: u32) -> Result<Value> {
        Ok(match self {
            Self::UInt(n) => Value::from_u64(width, *n),
            Self::Bool(b) => Value::from_u64(width, u64::from(*b)),
            Self::Bits(s) => s.parse::<Value>().map_err(error)?.resized(width),
        })
    }
}
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
struct Step {
    drive: BTreeMap<String, Literal>,
    clocks: Vec<String>,
    expect: BTreeMap<String, Literal>,
}

struct Waves {
    writer: Option<VcdWriter<BufWriter<File>>>,
    time: u64,
}
impl Waves {
    fn sample(&mut self, sim: &Simulator) -> Result<()> {
        if let Some(w) = &mut self.writer {
            w.sample(self.time, sim).map_err(error)?;
        }
        self.time += 1;
        Ok(())
    }
    fn pulse(&mut self, sim: &mut Simulator, clocks: &[DomainId]) -> Result<()> {
        sim.eval_comb().map_err(runtime)?;
        self.sample(sim)?;
        if !clocks.is_empty() {
            for edge in [ClockEvent::Rising, ClockEvent::Falling] {
                let group = clocks.iter().copied().map(edge).collect::<Vec<_>>();
                sim.step_clocks(&group).map_err(runtime)?;
                self.sample(sim)?;
            }
        }
        Ok(())
    }
}

pub(super) fn run<'a>(
    design: &SimDesign,
    args: &SimArgs,
    protected: impl Iterator<Item = &'a Path>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<()> {
    let mut sim = Simulator::new(design);
    sim.strict_unique = args.strict_unique;
    let steps = args
        .stimulus
        .as_ref()
        .map(|path| {
            let bytes = fs::read(path).map_err(|e| error(format!("{}: {e}", path.display())))?;
            serde_json::from_slice::<Vec<Step>>(&bytes)
                .map_err(|e| error(format!("{}: {e}", path.display())))
        })
        .transpose()?;
    let mut waves = Waves {
        time: 0,
        writer: args
            .vcd
            .as_ref()
            .map(|path| {
                let f = wave_file(path, protected)?;
                VcdWriter::new(BufWriter::new(f), design).map_err(error)
            })
            .transpose()?,
    };
    let top_input = |id| {
        design
            .top_ports()
            .iter()
            .any(|p| p.signal == id && p.direction != HwDirection::Out)
    };
    let mut clocks = Vec::new();
    for p in design
        .top_ports()
        .iter()
        .filter(|p| p.direction != HwDirection::Out)
    {
        if let Ok(id) = sim.clock(&p.name) {
            sim.drive(p.signal, Value::zero(1)).map_err(runtime)?;
            clocks.push(id);
        }
    }
    if steps.is_none()
        && args.cycles > 0
        && clocks.is_empty()
        && let Some(c) = design.clocks.first()
    {
        return Err(located(
            design.signal(c.signal).source.span(),
            "automatic stepping requires a top clock input; use --stimulus to drive the derived clock inputs",
        ));
    }
    let check = |sim: &Simulator| -> Result<()> {
        if args.fail_on_x
            && let Some(p) = design
                .top_ports()
                .iter()
                .find(|p| p.direction != HwDirection::In && !sim.read(p.signal).is_known())
        {
            return Err(located(
                design.signal(p.signal).source.span(),
                format!(
                    "cycle {}: unknown output {}={}",
                    sim.cycle(),
                    p.name,
                    sim.read(p.signal)
                ),
            ));
        }
        Ok(())
    };
    if let Some(steps) = steps {
        if steps.is_empty() {
            sim.eval_comb().map_err(runtime)?;
            waves.sample(&sim)?;
            check(&sim)?
        }
        for (i, step) in steps.into_iter().enumerate() {
            for (name, literal) in step.drive {
                let id = sim.port(&name).map_err(runtime)?;
                if sim.clock(&name).is_ok() {
                    return Err(error(format!("step {i}: use clocks to pulse {name}")));
                }
                sim.drive(id, literal.value(design.signal(id).width)?)
                    .map_err(runtime)?;
            }
            let clocks = step
                .clocks
                .iter()
                .map(|n| sim.clock(n))
                .collect::<std::result::Result<Vec<_>, _>>()
                .map_err(runtime)?;
            waves.pulse(&mut sim, &clocks)?;
            for (name, literal) in step.expect {
                let id = sim.port(&name).map_err(runtime)?;
                let expected = literal.value(design.signal(id).width)?;
                if sim.read(id) != &expected {
                    return Err(located(
                        design.signal(id).source.span(),
                        format!(
                            "step {i}, {}.{name}: expected {expected}, observed {}",
                            args.top,
                            sim.read(id)
                        ),
                    ));
                }
            }
            check(&sim)?;
        }
    } else {
        for p in design
            .top_ports()
            .iter()
            .filter(|p| p.direction != HwDirection::Out)
        {
            sim.drive(p.signal, Value::zero(design.signal(p.signal).width))
                .map_err(runtime)?;
        }
        for i in 0..args.cycles {
            for r in design.resets.iter().filter(|r| top_input(r.signal)) {
                let high = (i == 0) == (r.polarity == HwResetPolarity::ActiveHigh);
                sim.drive(r.signal, Value::from_u64(1, high.into()))
                    .map_err(runtime)?;
            }
            waves.pulse(&mut sim, &clocks)?;
            check(&sim)?;
        }
        if args.cycles == 0 {
            sim.eval_comb().map_err(runtime)?;
            waves.sample(&sim)?;
            check(&sim)?
        }
    }
    if let Some(writer) = waves.writer {
        writer.finish().map_err(error)?;
    }
    for warning in &sim.unique_violations {
        diagnostics.push(
            Diagnostic::new(warning.span, warning.to_string())
                .with_code("W_SIM_UNIQUE")
                .with_severity(syl_span::DiagnosticSeverity::Warning)
                .with_source("sylc"),
        );
    }
    println!("{} cycles={}", args.top, sim.cycle());
    for p in design
        .top_ports()
        .iter()
        .filter(|p| p.direction != HwDirection::In)
    {
        println!("{}={}", p.name, sim.read(p.signal))
    }
    Ok(())
}

fn wave_file<'a>(path: &Path, protected: impl Iterator<Item = &'a Path>) -> Result<File> {
    // Compare the identity of the open destination before truncating it.
    // This includes hard links and keeps writes tied to the checked handle.
    let file = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(error)?;
    let destination =
        same_file::Handle::from_file(file.try_clone().map_err(error)?).map_err(error)?;
    for input in protected {
        if same_file::Handle::from_path(input).map_err(error)? == destination {
            return Err(error(format!(
                "VCD destination {} refers to input {}",
                path.display(),
                input.display()
            )));
        }
    }
    file.set_len(0).map_err(error)?;
    Ok(file)
}
