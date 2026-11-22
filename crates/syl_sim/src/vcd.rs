//! Hierarchical four-state Value Change Dump output.
use crate::{SignalId, SimDesign, Simulator, Value};
use std::{
    collections::BTreeMap,
    io::{self, Write},
};

#[derive(Default)]
struct Scope<'a> {
    children: BTreeMap<&'a str, Self>,
    nets: Vec<(SignalId, u32, &'a str)>,
}
impl Scope<'_> {
    fn header(&self, out: &mut impl Write) -> io::Result<()> {
        for (id, width, name) in &self.nets {
            writeln!(out, "$var wire {width} s{} {name} $end", id.0)?;
        }
        for (name, scope) in &self.children {
            writeln!(out, "$scope module {name} $end")?;
            scope.header(out)?;
            writeln!(out, "$upscope $end")?;
        }
        Ok(())
    }
}
pub struct VcdWriter<W> {
    out: W,
    tracked: Vec<(SignalId, Option<Value>)>,
    last_time: Option<u64>,
}
impl<W: Write> VcdWriter<W> {
    pub fn new(out: W, design: &SimDesign) -> io::Result<Self> {
        Self::filtered(out, design, |_, _| true)
    }
    /// The predicate receives each signal ID and full Syl path. Select an ID
    /// set or a hierarchy prefix without allocating storage for excluded nets.
    pub fn filtered(
        mut out: W,
        design: &SimDesign,
        mut include: impl FnMut(SignalId, &str) -> bool,
    ) -> io::Result<Self> {
        let mut root = Scope::default();
        let mut tracked = Vec::new();
        for (i, s) in design.signals.iter().enumerate() {
            let id = SignalId(i);
            if !include(id, &s.source.path) {
                continue;
            }
            let mut scope = &mut root;
            let mut parts = s.source.path.split('.').peekable();
            while let Some(name) = parts.next() {
                if parts.peek().is_none() {
                    scope.nets.push((id, s.width, name));
                    break;
                }
                scope = scope.children.entry(name).or_default();
            }
            tracked.push((id, None));
        }
        writeln!(out, "$version syl_sim $end\n$timescale 1ns $end")?;
        root.header(&mut out)?;
        writeln!(out, "$enddefinitions $end")?;
        Ok(Self {
            out,
            tracked,
            last_time: None,
        })
    }
    pub fn sample(&mut self, time: u64, sim: &Simulator<'_>) -> io::Result<()> {
        if self.last_time.is_some_and(|t| time < t) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "VCD time moved backwards",
            ));
        }
        if self.last_time != Some(time) {
            writeln!(self.out, "#{time}")?;
            self.last_time = Some(time)
        }
        for (id, last) in &mut self.tracked {
            let v = sim.read(*id);
            if last.as_ref() == Some(v) {
                continue;
            }
            if v.width() == 1 {
                writeln!(self.out, "{v}s{}", id.0)?
            } else {
                writeln!(self.out, "b{v} s{}", id.0)?
            }
            *last = Some(v.clone());
        }
        Ok(())
    }
    pub fn finish(mut self) -> io::Result<W> {
        self.out.flush()?;
        Ok(self.out)
    }
}
